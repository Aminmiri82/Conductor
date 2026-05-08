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

export function parseQueryEntries(url: string): Array<{ key: string; value: string }> {
  const query = extractQuery(url);
  if (!query) return [];
  const seen = new Set<string>();
  const out: Array<{ key: string; value: string }> = [];
  for (const part of query.split("&")) {
    if (!part) continue;
    const eq = part.indexOf("=");
    const rawKey = eq === -1 ? part : part.slice(0, eq);
    const rawValue = eq === -1 ? "" : part.slice(eq + 1);
    const key = decodeParam(rawKey);
    if (!key || seen.has(key)) continue;
    seen.add(key);
    out.push({ key, value: decodeParam(rawValue) });
  }
  return out;
}

export function deriveQueryRows(url: string, previous: KeyValue[]): KeyValue[] {
  const fromUrl = parseQueryEntries(url);
  const inUrl = new Set(fromUrl.map((entry) => entry.key));
  const rows: KeyValue[] = [];

  for (const entry of fromUrl) {
    const prior = previous.find((row) => row.key === entry.key);
    rows.push({
      key: entry.key,
      value: entry.value,
      enabled: prior?.enabled ?? true,
    });
  }

  for (const row of previous) {
    if (row.key && !inUrl.has(row.key) && row.enabled === false) {
      rows.push(row);
    }
  }

  for (const row of previous) {
    if (!row.key) rows.push(row);
  }

  return rows;
}

export function derivePathParamRows(url: string, previous: KeyValue[]): KeyValue[] {
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
    parts.push(`${encodeQueryComponent(row.key)}=${encodeQueryComponent(row.value)}`);
  }
  return parts.join("&");
}

export function replaceQueryInUrl(url: string, queryString: string): string {
  const qIdx = url.indexOf("?");
  const hIdx = url.indexOf("#", qIdx === -1 ? 0 : qIdx);
  const head =
    qIdx === -1
      ? hIdx === -1
        ? url
        : url.slice(0, hIdx)
      : url.slice(0, qIdx);
  const fragment = hIdx === -1 ? "" : url.slice(hIdx);
  if (!queryString) return head + fragment;
  return `${head}?${queryString}${fragment}`;
}

export function renamePathParamInUrl(
  url: string,
  oldKey: string,
  newKey: string,
): string {
  if (!oldKey || !newKey || oldKey === newKey) return url;
  const { path, tail } = splitPath(url);
  const re = new RegExp(`(^|/):${escapeRegExp(oldKey)}(?=/|$)`, "g");
  return path.replace(re, `$1:${newKey}`) + tail;
}

export function removePathParamFromUrl(url: string, key: string): string {
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
