import type {
  VariableChange,
  VariableEntry,
  VariableScope,
  VariableTarget,
} from "@/features/types";

// Editors send only the keys they changed (see `apply_variable_changes`), so
// saving never rewrites a value that someone else changed in the meantime.

export type RequestVariableRow = {
  key: string;
  value: string;
  scope: VariableScope;
  // The scope the variable is stored in now; null if it is not defined yet.
  originalScope: VariableScope | null;
  enabled: boolean;
  sensitive: boolean;
};

export type RemovedVariable = { scope: VariableScope; key: string };

/** Rust trims keys and ignores blank ones; the baseline must match. */
export function normalizeVariables(rows: readonly VariableEntry[]) {
  return rows
    .map((row) => ({ ...row, key: row.key.trim() }))
    .filter((row) => row.key);
}

/**
 * The change that turns a loaded table into the edited one. A key in the
 * table more than once is always sent, so Rust rejects the ambiguity rather
 * than one row silently winning.
 */
export function diffVariables(
  loaded: readonly VariableEntry[],
  current: readonly VariableEntry[],
) {
  const before = new Map(loaded.map((row) => [row.key, row]));
  const rows = normalizeVariables(current);
  const counts = new Map<string, number>();
  for (const { key } of rows) counts.set(key, (counts.get(key) ?? 0) + 1);

  return {
    upserts: rows.filter((row) => {
      const old = before.get(row.key);
      return (
        (counts.get(row.key) ?? 0) > 1 ||
        !old ||
        old.value !== row.value ||
        old.enabled !== row.enabled ||
        old.sensitive !== row.sensitive
      );
    }),
    deletes: loaded.map((row) => row.key).filter((key) => !counts.has(key)),
  };
}

/**
 * The changes behind the request's Variables tab: edited rows, rows that
 * have no stored variable yet, rows moved to another scope (deleted from the
 * old one), and removed rows.
 */
export function requestVariableChanges({
  rows,
  edited,
  removed,
  targetFor,
}: {
  rows: readonly RequestVariableRow[];
  edited: { has(key: string): boolean };
  removed: Iterable<RemovedVariable>;
  // Null when the scope cannot be saved to (no active environment).
  targetFor: (scope: VariableScope) => VariableTarget | null;
}): VariableChange[] {
  const byScope = new Map<
    VariableScope,
    { upserts: VariableEntry[]; deletes: string[] }
  >();
  const scopeChange = (scope: VariableScope) => {
    let change = byScope.get(scope);
    if (!change) byScope.set(scope, (change = { upserts: [], deletes: [] }));
    return change;
  };

  for (const { scope, key } of removed) scopeChange(scope).deletes.push(key);
  for (const row of rows) {
    const { originalScope } = row;
    const moved = originalScope !== null && originalScope !== row.scope;
    if (moved) scopeChange(originalScope).deletes.push(row.key);
    if (moved || originalScope === null || edited.has(row.key)) {
      scopeChange(row.scope).upserts.push({
        key: row.key,
        value: row.value,
        enabled: row.enabled,
        sensitive: row.sensitive,
      });
    }
  }

  return [...byScope].flatMap(([scope, change]) => {
    const target = targetFor(scope);
    return target ? [{ target, ...change }] : [];
  });
}
