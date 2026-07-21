import type {
  AuthConfig,
  KeyValue,
  RequestBody,
  RequestDetail,
  ResolvedRequestPreview,
} from "@/features/types";

export type CurlInput = {
  method: string;
  request: RequestDetail;
  preview?: ResolvedRequestPreview;
  /** When true, auth secrets and sensitive header/query values become `[REDACTED]`. */
  redactSecrets?: boolean;
};

const REDACTED = "[REDACTED]";

/**
 * Build a `curl` command that reproduces the request as it would be sent.
 * When a resolved preview is available its URL/headers/body take precedence
 * so variables are substituted.
 */
export function buildCurlCommand({
  method,
  request,
  preview,
  redactSecrets = false,
}: CurlInput): string {
  const auth = request.effectiveAuth ?? request.auth ?? null;
  let url = preview?.url ?? applyQueryFallback(request);
  if (redactSecrets) {
    url = redactUrlQuery(url);
  }

  // Apply API-key-as-query the same way the backend send path does.
  if (auth?.authType === "apikey" && auth.addTo === "query" && auth.key) {
    const value = redactSecrets ? REDACTED : (auth.value ?? "");
    url = appendQueryParam(url, auth.key, value);
  }

  const method_ = method.toUpperCase();
  const headers = mergeHeaders(
    preview?.headers ?? request.headers ?? [],
    effectiveAuthHeaders(auth, redactSecrets),
  );
  const body = preview?.body ?? request.body ?? null;

  const parts: string[] = ["curl"];
  if (method_ !== "GET") {
    parts.push("-X", shellQuote(method_));
  }
  parts.push(shellQuote(url));

  const headerNames = new Set<string>();
  for (const header of headers) {
    if (!header.enabled) continue;
    if (!header.key.trim()) continue;
    const value =
      redactSecrets && isSensitiveHeader(header.key) ? REDACTED : header.value;
    parts.push("-H", shellQuote(`${header.key}: ${value}`));
    headerNames.add(header.key.toLowerCase());
  }

  if (auth?.authType === "basic") {
    const username = auth.username ?? "";
    const password = redactSecrets ? REDACTED : (auth.password ?? "");
    parts.push("-u", shellQuote(`${username}:${password}`));
  }

  const bodyPieces = bodyToCurlArgs(body, method_, headerNames, redactSecrets);
  parts.push(...bodyPieces);

  return parts.join(" ");
}

/** True when copying this request would put secrets on the clipboard. */
export function curlContainsSecrets(request: RequestDetail): boolean {
  const auth = request.effectiveAuth ?? request.auth ?? null;
  if (auth) {
    if (auth.authType === "bearer" && auth.token) return true;
    if (auth.authType === "basic" && auth.password) return true;
    if (auth.authType === "apikey" && auth.value) return true;
  }
  for (const header of request.headers ?? []) {
    if (header.enabled && header.value && isSensitiveHeader(header.key)) {
      return true;
    }
  }
  for (const query of request.query ?? []) {
    if (query.enabled && query.value && isSensitiveParam(query.key)) {
      return true;
    }
  }
  return /[?&](api[_-]?key|token|secret|password|auth)=/i.test(request.url);
}

export async function copyCurlToClipboard(command: string): Promise<void> {
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(command);
    return;
  }
  throw new Error("Clipboard API is unavailable in this environment.");
}

function applyQueryFallback(request: RequestDetail): string {
  const enabled = (request.query ?? []).filter(
    (entry) => entry.enabled && entry.key.trim(),
  );
  if (!enabled.length) return request.url;
  try {
    const parsed = new URL(request.url);
    for (const entry of enabled) {
      parsed.searchParams.set(entry.key, entry.value);
    }
    return parsed.toString();
  } catch {
    const base = request.url.split("?")[0] ?? request.url;
    const qs = enabled
      .map(
        (entry) =>
          `${encodeURIComponent(entry.key)}=${encodeURIComponent(entry.value)}`,
      )
      .join("&");
    return `${base}?${qs}`;
  }
}

function appendQueryParam(url: string, key: string, value: string): string {
  try {
    const parsed = new URL(url);
    parsed.searchParams.set(key, value);
    return parsed.toString();
  } catch {
    const sep = url.includes("?") ? "&" : "?";
    return `${url}${sep}${encodeURIComponent(key)}=${encodeURIComponent(value)}`;
  }
}

function redactUrlQuery(url: string): string {
  const qIndex = url.indexOf("?");
  if (qIndex < 0) return url;
  const base = url.slice(0, qIndex);
  const query = url.slice(qIndex + 1);
  if (!query) return url;
  const redacted = query
    .split("&")
    .map((pair) => {
      const eq = pair.indexOf("=");
      if (eq < 0) return pair;
      const key = pair.slice(0, eq);
      const value = pair.slice(eq + 1);
      return isSensitiveParam(decodeURIComponent(key))
        ? `${key}=${REDACTED}`
        : `${key}=${value}`;
    })
    .join("&");
  return `${base}?${redacted}`;
}

function mergeHeaders(headers: KeyValue[], extras: KeyValue[]): KeyValue[] {
  if (!extras.length) return headers;
  const seen = new Set(
    headers
      .filter((header) => header.enabled)
      .map((header) => header.key.toLowerCase()),
  );
  const merged = [...headers];
  for (const extra of extras) {
    if (seen.has(extra.key.toLowerCase())) continue;
    merged.push(extra);
  }
  return merged;
}

function effectiveAuthHeaders(
  auth: AuthConfig | null,
  redactSecrets: boolean,
): KeyValue[] {
  if (!auth) return [];
  switch (auth.authType) {
    case "bearer":
      if (!auth.token) return [];
      return [
        {
          key: "Authorization",
          value: redactSecrets ? `Bearer ${REDACTED}` : `Bearer ${auth.token}`,
          enabled: true,
        },
      ];
    case "apikey":
      if (!auth.key || auth.addTo === "query") return [];
      return [
        {
          key: auth.key,
          value: redactSecrets ? REDACTED : (auth.value ?? ""),
          enabled: true,
        },
      ];
    default:
      return [];
  }
}

function bodyToCurlArgs(
  body: RequestBody | null,
  method: string,
  existingHeaderNames: Set<string>,
  redactSecrets: boolean,
): string[] {
  if (!body || body.mode === "none") return [];

  const pushContentType = (args: string[], contentType: string) => {
    if (existingHeaderNames.has("content-type")) return;
    args.unshift("-H", shellQuote(`Content-Type: ${contentType}`));
  };

  switch (body.mode) {
    case "raw": {
      if (!body.raw) return [];
      const language = body.rawLanguage?.toLowerCase();
      const raw =
        redactSecrets && looksLikeSecretPayload(body.raw) ? REDACTED : body.raw;
      const args = ["--data-raw", shellQuote(raw)];
      if (language === "json") {
        pushContentType(args, "application/json");
      } else if (language === "xml") {
        pushContentType(args, "application/xml");
      }
      return args;
    }
    case "urlencoded": {
      const args: string[] = [];
      for (const entry of body.urlencoded ?? []) {
        if (!entry.enabled || !entry.key) continue;
        const value =
          redactSecrets && isSensitiveParam(entry.key) ? REDACTED : entry.value;
        args.push("--data-urlencode", shellQuote(`${entry.key}=${value}`));
      }
      return args;
    }
    case "formdata": {
      const args: string[] = [];
      for (const field of body.formData ?? []) {
        if (!field.enabled || !field.key) continue;
        if (field.fieldType === "file" && field.filePath) {
          args.push("-F", shellQuote(`${field.key}=@${field.filePath}`));
        } else {
          const value =
            redactSecrets && isSensitiveParam(field.key)
              ? REDACTED
              : field.value;
          args.push("-F", shellQuote(`${field.key}=${value}`));
        }
      }
      return args;
    }
    case "graphql": {
      const variablesRaw = body.graphql?.variables ?? "";
      const payload = JSON.stringify({
        query: body.graphql?.query ?? "",
        variables:
          redactSecrets && looksLikeSecretPayload(variablesRaw)
            ? REDACTED
            : safeParseJson(variablesRaw),
      });
      const args = ["--data-raw", shellQuote(payload)];
      pushContentType(args, "application/json");
      return args;
    }
    case "file": {
      if (!body.file?.path) return [];
      const args = ["--data-binary", shellQuote(`@${body.file.path}`)];
      if (body.file.contentType) {
        pushContentType(args, body.file.contentType);
      }
      return args;
    }
    default: {
      if (method === "GET") return [];
      return [];
    }
  }
}

function safeParseJson(value: string): unknown {
  const trimmed = value.trim();
  if (!trimmed) return {};
  try {
    return JSON.parse(trimmed);
  } catch {
    return trimmed;
  }
}

function isSensitiveHeader(name: string): boolean {
  const lower = name.trim().toLowerCase();
  return (
    lower === "authorization" ||
    lower === "proxy-authorization" ||
    lower === "cookie" ||
    lower === "x-api-key" ||
    lower === "api-key" ||
    lower.includes("api-key") ||
    lower.includes("apikey") ||
    lower.endsWith("-token") ||
    lower.endsWith("-secret") ||
    lower.endsWith("-password")
  );
}

function isSensitiveParam(name: string): boolean {
  const lower = name.trim().toLowerCase().replace(/_/g, "-");
  return (
    [
      "access-token",
      "api-key",
      "apikey",
      "auth",
      "authorization",
      "key",
      "password",
      "passwd",
      "secret",
      "token",
      "refresh-token",
      "client-secret",
      "private-key",
    ].includes(lower) ||
    lower.includes("api-key") ||
    lower.includes("apikey") ||
    lower.endsWith("-token") ||
    lower.endsWith("-secret") ||
    lower.endsWith("-password")
  );
}

function looksLikeSecretPayload(raw: string): boolean {
  const lower = raw.toLowerCase();
  return (
    lower.includes('"password"') ||
    lower.includes('"client_secret"') ||
    lower.includes('"access_token"') ||
    lower.includes('"api_key"') ||
    lower.includes('"api-key"')
  );
}

/**
 * POSIX-style single-quote escaping so the produced command is safe to paste
 * into bash/zsh terminals.
 */
export function shellQuote(value: string): string {
  if (value === "") return "''";
  if (/^[a-zA-Z0-9_@%+=:,./-]+$/.test(value)) return value;
  return `'${value.replace(/'/g, `'\\''`)}'`;
}
