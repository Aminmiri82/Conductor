import type {
  VariableChange,
  VariableEntry,
  VariableScope,
  VariableTarget,
} from "@/features/types";

// Editors send only the keys they changed (see `apply_variable_changes`), so
// saving never rewrites a value that someone else changed in the meantime.

export type RequestVariableRow = Pick<
  VariableEntry,
  "origin" | "hasSharedValue" | "requiredSecret"
> & {
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

/** Keys (trimmed) used by more than one row, which Rust would reject. */
export function duplicateKeys(rows: readonly VariableEntry[]) {
  const seen = new Set<string>();
  const duplicates = new Set<string>();
  for (const { key } of normalizeVariables(rows)) {
    if (seen.has(key)) duplicates.add(key);
    seen.add(key);
  }
  return duplicates;
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

export type ScopedVariables = Record<VariableScope, VariableEntry[]>;

/**
 * One row per name a request references: the stored variable that wins
 * (environment, then collection, then global), or a blank row in the scope a
 * new variable would be saved to.
 */
export function requestVariableRows(
  keys: readonly string[],
  stored: ScopedVariables,
  hasEnvironment: boolean,
): RequestVariableRow[] {
  const byKey = (variables: VariableEntry[]) =>
    new Map(variables.map((variable) => [variable.key, variable]));
  const environment = byKey(stored.environment);
  const collection = byKey(stored.collection);
  const global = byKey(stored.global);

  return keys.map((key) => {
    const variable =
      environment.get(key) ?? collection.get(key) ?? global.get(key);
    const scope = environment.has(key)
      ? "environment"
      : collection.has(key)
        ? "collection"
        : global.has(key)
          ? "global"
          : hasEnvironment
            ? "environment"
            : "collection";
    return {
      key,
      value: variable?.value ?? "",
      scope,
      originalScope: variable ? scope : null,
      enabled: variable?.enabled ?? true,
      sensitive: variable?.sensitive ?? false,
    };
  });
}

/**
 * What the request's Variables tab saves: each edited row is set in its
 * scope (and deleted from the scope it was moved out of), and removed
 * variables are deleted. A row nobody edited is not sent, so a name the
 * request references stays undefined until it is given a value.
 */
export function requestVariableChanges({
  edited,
  removed,
  targetFor,
}: {
  edited: readonly RequestVariableRow[];
  removed: readonly RemovedVariable[];
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
  for (const row of edited) {
    const { originalScope } = row;
    if (originalScope !== null && originalScope !== row.scope) {
      scopeChange(originalScope).deletes.push(row.key);
    }
    scopeChange(row.scope).upserts.push({
      key: row.key,
      value: row.value,
      enabled: row.enabled,
      sensitive: row.sensitive,
    });
  }

  return [...byScope].flatMap(([scope, change]) => {
    const target = targetFor(scope);
    return target ? [{ target, ...change }] : [];
  });
}

/**
 * What is stored once `requestVariableChanges` for the same rows is saved,
 * so the tab does not list every scope again after each save. Deletes apply
 * first, as in Rust.
 */
export function applyRequestVariableChanges(
  stored: ScopedVariables,
  edited: readonly RequestVariableRow[],
  removed: readonly RemovedVariable[],
): ScopedVariables {
  const next = { ...stored };
  const drop = (scope: VariableScope, key: string) => {
    next[scope] = next[scope].filter((variable) => variable.key !== key);
  };

  for (const { scope, key } of removed) drop(scope, key);
  for (const row of edited) {
    if (row.originalScope !== null) drop(row.originalScope, row.key);
    drop(row.scope, row.key);
    next[row.scope] = [
      ...next[row.scope],
      {
        key: row.key,
        value: row.value,
        enabled: row.enabled,
        sensitive: row.sensitive,
      },
    ];
  }
  return next;
}
