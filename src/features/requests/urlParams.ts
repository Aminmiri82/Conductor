import type { KeyValue } from "@/features/types";

const PATH_TOKEN_RE = /(^|\/):([A-Za-z_][A-Za-z0-9_]*)/g;

export function parsePathParamKeys(url: string): string[] {
  const pathPart = stripQuery(url);
  const seen = new Set<string>();
  const out: string[] = [];
  let match: RegExpExecArray | null;
  PATH_TOKEN_RE.lastIndex = 0;
  while ((match = PATH_TOKEN_RE.exec(pathPart))) {
    const name = match[2];
    if (!seen.has(name)) {
      seen.add(name);
      out.push(name);
    }
  }
  return out;
}

function parseQueryEntries(url: string): Array<{ key: string; value: string }> {
  return extractQuery(url)
    .split("&")
    .flatMap((part) => parseQueryPart(part) ?? []);
}

function parseQueryPart(part: string): { key: string; value: string } | null {
  const eq = part.indexOf("=");
  const rawKey = eq === -1 ? part : part.slice(0, eq);
  const rawValue = eq === -1 ? "" : part.slice(eq + 1);
  const key = decodeParam(rawKey);
  return key ? { key, value: decodeParam(rawValue) } : null;
}

// The URL holds exactly the enabled rows, so its entries become the enabled
// rows and disabled rows carry over. Repeated keys stay separate rows.
export function deriveQueryRows(url: string, previous: KeyValue[]): KeyValue[] {
  return [
    ...parseQueryEntries(url).map((entry) => ({ ...entry, enabled: true })),
    ...previous.filter((row) => row.key && row.enabled === false),
    ...previous.filter((row) => !row.key),
  ];
}

// For a request loaded from storage, whose URL may still list disabled rows
// (Postman exports keep them): each URL entry takes the enabled flag of the
// stored row with the same key at the same occurrence, and disabled entries
// are cut from the URL so it holds exactly the enabled rows, as
// `deriveQueryRows` expects. Kept entries stay byte-for-byte as written.
export function loadQuery(
  url: string,
  stored: KeyValue[],
): { url: string; query: KeyValue[] } {
  const unmatched = [...stored];
  const rows: KeyValue[] = [];
  const keptParts: string[] = [];
  let droppedAny = false;
  for (const part of extractQuery(url).split("&")) {
    const entry = parseQueryPart(part);
    if (!entry) {
      if (part) keptParts.push(part);
      continue;
    }
    const index = unmatched.findIndex((row) => row.key === entry.key);
    const enabled = index === -1 ? true : unmatched[index].enabled;
    if (index !== -1) unmatched.splice(index, 1);
    rows.push({ ...entry, enabled });
    if (enabled) keptParts.push(part);
    else droppedAny = true;
  }
  return {
    url: droppedAny ? replaceQueryInUrl(url, keptParts.join("&")) : url,
    query: [
      ...rows,
      ...unmatched.filter((row) => row.key && row.enabled === false),
      ...unmatched.filter((row) => !row.key),
    ],
  };
}

export function derivePathParamRows(
  url: string,
  previous: KeyValue[],
): KeyValue[] {
  const keys = parsePathParamKeys(url);
  return keys.map((key) => {
    const prior = previous.find((row) => row.key === key);
    return {
      key,
      value: prior?.value ?? "",
      enabled: prior?.enabled ?? true,
    };
  });
}

export function buildQueryString(rows: KeyValue[]): string {
  const parts: string[] = [];
  for (const row of rows) {
    if (!row.key) continue;
    if (row.enabled === false) continue;
    parts.push(
      `${encodeQueryComponent(row.key)}=${encodeQueryComponent(row.value)}`,
    );
  }
  return parts.join("&");
}

export function replaceQueryInUrl(url: string, queryString: string): string {
  const qIdx = url.indexOf("?");
  const hIdx = url.indexOf("#", qIdx === -1 ? 0 : qIdx);
  const head =
    qIdx === -1 ? (hIdx === -1 ? url : url.slice(0, hIdx)) : url.slice(0, qIdx);
  const fragment = hIdx === -1 ? "" : url.slice(hIdx);
  if (!queryString) return head + fragment;
  return `${head}?${queryString}${fragment}`;
}

function renamePathParamInUrl(
  url: string,
  oldKey: string,
  newKey: string,
): string {
  if (!oldKey || !newKey || oldKey === newKey) return url;
  const { path, tail } = splitPath(url);
  const re = new RegExp(`(^|/):${escapeRegExp(oldKey)}(?=/|$)`, "g");
  return path.replace(re, `$1:${newKey}`) + tail;
}

function removePathParamFromUrl(url: string, key: string): string {
  if (!key) return url;
  const { path, tail } = splitPath(url);
  const re = new RegExp(`(^|/):${escapeRegExp(key)}(?=/|$)`, "g");
  return path.replace(re, "") + tail;
}

export function applyPathParamRowChanges(
  url: string,
  previous: KeyValue[],
  next: KeyValue[],
): string {
  let result = url;

  const minLen = Math.min(previous.length, next.length);
  for (let i = 0; i < minLen; i++) {
    const prev = previous[i];
    const curr = next[i];
    if (prev.key && curr.key && prev.key !== curr.key) {
      result = renamePathParamInUrl(result, prev.key, curr.key);
    }
  }

  if (next.length < previous.length) {
    const nextKeys = new Set(next.map((row) => row.key));
    for (const row of previous) {
      if (row.key && !nextKeys.has(row.key)) {
        result = removePathParamFromUrl(result, row.key);
      }
    }
  }

  return result;
}

function stripQuery(url: string): string {
  const i = url.search(/[?#]/);
  return i === -1 ? url : url.slice(0, i);
}

function extractQuery(url: string): string {
  const q = url.indexOf("?");
  if (q === -1) return "";
  const h = url.indexOf("#", q);
  return url.slice(q + 1, h === -1 ? undefined : h);
}

function splitPath(url: string): { path: string; tail: string } {
  const i = url.search(/[?#]/);
  if (i === -1) return { path: url, tail: "" };
  return { path: url.slice(0, i), tail: url.slice(i) };
}

function decodeParam(value: string): string {
  try {
    return decodeURIComponent(value.replace(/\+/g, " "));
  } catch {
    return value;
  }
}

function encodeQueryComponent(value: string): string {
  return encodeURIComponent(value).replace(/%20/g, "+");
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
