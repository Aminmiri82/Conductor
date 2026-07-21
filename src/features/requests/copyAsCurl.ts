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
};

/**
 * Build a `curl` command that reproduces the request as it would be sent.
 * When a resolved preview is available its URL/headers/body take precedence
 * so variables are substituted.
 */
export function buildCurlCommand({
  method,
  request,
  preview,
}: CurlInput): string {
  const url = preview?.url ?? applyQueryFallback(request);
  const method_ = method.toUpperCase();
  const headers = mergeHeaders(
    preview?.headers ?? request.headers ?? [],
    effectiveAuthHeaders(request.effectiveAuth ?? request.auth ?? null),
  );
  const body = preview?.body ?? request.body ?? null;

  const parts: string[] = ["curl"];
  if (method_ !== "GET") {
    parts.push("-X", shellQuote(method_));
  }
  parts.push(shellQuote(url));

  for (const header of headers) {
    if (!header.enabled) continue;
    if (!header.key.trim()) continue;
    parts.push("-H", shellQuote(`${header.key}: ${header.value}`));
  }

  const auth = request.effectiveAuth ?? request.auth ?? null;
  if (auth?.authType === "basic") {
    const username = auth.username ?? "";
    const password = auth.password ?? "";
    parts.push("-u", shellQuote(`${username}:${password}`));
  }

  const bodyPieces = bodyToCurlArgs(body, method_);
  parts.push(...bodyPieces);

  return parts.join(" ");
}

export async function copyCurlToClipboard(command: string): Promise<void> {
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(command);
    return;
  }
  throw new Error("Clipboard API is unavailable in this environment.");
}

function applyQueryFallback(request: RequestDetail): string {
  return request.url;
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

function effectiveAuthHeaders(auth: AuthConfig | null): KeyValue[] {
  if (!auth) return [];
  switch (auth.authType) {
    case "bearer":
      if (!auth.token) return [];
      return [
        {
          key: "Authorization",
          value: `Bearer ${auth.token}`,
          enabled: true,
        },
      ];
    case "apikey":
      if (!auth.key || auth.addTo === "query") return [];
      return [
        {
          key: auth.key,
          value: auth.value ?? "",
          enabled: true,
        },
      ];
    default:
      return [];
  }
}

function bodyToCurlArgs(body: RequestBody | null, method: string): string[] {
  if (!body || body.mode === "none") return [];

  switch (body.mode) {
    case "raw": {
      if (!body.raw) return [];
      const language = body.rawLanguage?.toLowerCase();
      const args = ["--data-raw", shellQuote(body.raw)];
      if (language === "json") {
        args.unshift("-H", shellQuote("Content-Type: application/json"));
      } else if (language === "xml") {
        args.unshift("-H", shellQuote("Content-Type: application/xml"));
      }
      return args;
    }
    case "urlencoded": {
      const args: string[] = [];
      for (const entry of body.urlencoded ?? []) {
        if (!entry.enabled || !entry.key) continue;
        args.push(
          "--data-urlencode",
          shellQuote(`${entry.key}=${entry.value}`),
        );
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
          args.push("-F", shellQuote(`${field.key}=${field.value}`));
        }
      }
      return args;
    }
    case "graphql": {
      const payload = JSON.stringify({
        query: body.graphql?.query ?? "",
        variables: safeParseJson(body.graphql?.variables ?? ""),
      });
      return [
        "-H",
        shellQuote("Content-Type: application/json"),
        "--data-raw",
        shellQuote(payload),
      ];
    }
    case "file": {
      if (!body.file?.path) return [];
      const args = ["--data-binary", shellQuote(`@${body.file.path}`)];
      if (body.file.contentType) {
        args.unshift(
          "-H",
          shellQuote(`Content-Type: ${body.file.contentType}`),
        );
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

/**
 * POSIX-style single-quote escaping so the produced command is safe to paste
 * into bash/zsh terminals.
 */
export function shellQuote(value: string): string {
  if (value === "") return "''";
  if (/^[a-zA-Z0-9_@%+=:,./-]+$/.test(value)) return value;
  return `'${value.replace(/'/g, `'\\''`)}'`;
}
