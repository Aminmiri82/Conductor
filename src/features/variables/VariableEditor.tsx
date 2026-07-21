import { useEffect, useMemo, useState } from "react";
import { Save, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { api } from "@/lib/tauri";
import type { EntityId, KeyValue, RequestDetail, VariableEntry } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";

type VariableScope = "environment" | "collection" | "global";

type RequestVariableRow = {
  key: string;
  value: string;
  scope: VariableScope;
  originalScope: VariableScope | null;
  enabled: boolean;
  sensitive: boolean;
};

export function VariableEditor({ request }: { request: RequestDetail }) {
  const variableNames = useMemo(() => extractRequestVariableNames(request), [request]);
  const [globalVariables, setGlobalVariables] = useState<VariableEntry[]>([]);
  const [collectionVariables, setCollectionVariables] = useState<VariableEntry[]>([]);
  const [environmentVariables, setEnvironmentVariables] = useState<VariableEntry[]>([]);
  const [rows, setRows] = useState<RequestVariableRow[]>([]);
  const [removedVariables, setRemovedVariables] = useState<Set<string>>(new Set());
  const [savedAt, setSavedAt] = useState<number>();
  const [saving, setSaving] = useState(false);
  const resolveActiveRequest = useWorkspaceStore((state) => state.resolveActiveRequest);
  const activeEnvironmentId = useWorkspaceUiStore(
    (state) => state.workspaceUi.activeEnvironmentId,
  );
  const environments = useWorkspaceStore((state) => state.environments);
  const activeEnvironmentName = environments.find(
    (environment) => environment.id === activeEnvironmentId,
  )?.name;

  useEffect(() => {
    let mounted = true;

    void Promise.all([
      api.listVariables("global", null, null),
      api.listVariables("collection", request.collectionId, null),
      activeEnvironmentId
        ? api.listVariables("environment", null, activeEnvironmentId)
        : Promise.resolve([]),
    ]).then(([globals, collection, environment]) => {
      if (!mounted) return;
      setGlobalVariables(globals);
      setCollectionVariables(collection);
      setEnvironmentVariables(environment);
      setRows(buildRows(variableNames, globals, collection, environment, activeEnvironmentId));
      setRemovedVariables(new Set());
      setSavedAt(undefined);
    });

    return () => {
      mounted = false;
    };
  }, [activeEnvironmentId, request.collectionId, variableNames]);

  async function save() {
    setSaving(true);
    const nextGlobalVariables = mergeVariables(
      globalVariables,
      rows,
      "global",
      null,
      null,
      removedVariables,
    );
    const nextCollectionVariables = mergeVariables(
      collectionVariables,
      rows,
      "collection",
      request.collectionId,
      null,
      removedVariables,
    );
    const nextEnvironmentVariables = activeEnvironmentId
      ? mergeVariables(
          environmentVariables,
          rows,
          "environment",
          null,
          activeEnvironmentId,
          removedVariables,
        )
      : environmentVariables;

    await api.saveVariables("global", null, null, nextGlobalVariables);
    await api.saveVariables(
      "collection",
      request.collectionId,
      null,
      nextCollectionVariables,
    );
    if (activeEnvironmentId) {
      await api.saveVariables(
        "environment",
        null,
        activeEnvironmentId,
        nextEnvironmentVariables,
      );
    }
    setGlobalVariables(nextGlobalVariables);
    setCollectionVariables(nextCollectionVariables);
    setEnvironmentVariables(nextEnvironmentVariables);
    setRemovedVariables(new Set());
    setSaving(false);
    setSavedAt(Date.now());
    await resolveActiveRequest();
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex min-h-12 items-center justify-between gap-3 border-b border-[var(--app-line)] px-4 py-2">
        <div className="min-w-0 text-xs text-[var(--app-dim)]">
          Shows variables referenced in this request. Manage the full list in Settings.
          {activeEnvironmentName ? ` Active environment: ${activeEnvironmentName}.` : ""}
        </div>
        <div className="flex items-center gap-2">
          <span className="w-16 text-right text-xs text-[var(--app-dim)]">
            {saving ? "Saving" : savedAt ? "Saved" : ""}
          </span>
          <Button
            className="h-8 gap-1.5 bg-[var(--app-accent)] px-3 text-xs text-[var(--app-accent-fg)] hover:bg-[var(--app-accent)]/90"
            onClick={save}
            disabled={saving || rows.length === 0}
          >
            <Save className="size-3.5" />
            Save
          </Button>
        </div>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="p-4">
          <div className="overflow-hidden border border-[var(--app-line)] bg-[var(--app-panel)]">
            <div className="grid h-9 grid-cols-[32px_minmax(130px,.8fr)_112px_minmax(170px,1.2fr)_80px_44px] items-center border-b border-[var(--app-line)] bg-[rgb(255_255_255/.02)] px-1 text-[11px] uppercase tracking-[0.06em] text-[var(--app-dim)]">
              <div />
              <div>Key</div>
              <div>Scope</div>
              <div>Value</div>
              <div>Secret</div>
              <div />
            </div>
            {rows.length ? (
              rows.map((row, index) => (
                <div
                  key={row.key}
                  className="grid grid-cols-[32px_minmax(130px,.8fr)_112px_minmax(170px,1.2fr)_80px_44px] items-center border-b border-[var(--app-line)] px-1 last:border-b-0"
                >
                  <input
                    type="checkbox"
                    className="mx-auto size-3 accent-[var(--app-accent)]"
                    checked={row.enabled}
                    onChange={(event) =>
                      updateRow(index, { enabled: event.target.checked })
                    }
                  />
                  <div className="app-mono truncate px-2 text-xs text-[var(--app-text)]">
                    {row.key}
                  </div>
                  <Select
                    value={row.scope}
                    onValueChange={(scope) =>
                      updateRow(index, { scope: scope as VariableScope })
                    }
                  >
                    <SelectTrigger className="h-8 rounded-none border-0 bg-transparent px-2 text-xs shadow-none focus:ring-0">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {activeEnvironmentId ? (
                        <SelectItem value="environment">Environment</SelectItem>
                      ) : null}
                      <SelectItem value="collection">Collection</SelectItem>
                      <SelectItem value="global">Global</SelectItem>
                    </SelectContent>
                  </Select>
                  <Input
                    className="app-mono h-8 rounded-none border-0 bg-transparent text-xs shadow-none focus-visible:ring-0"
                    value={row.value}
                    type={row.sensitive ? "password" : "text"}
                    onChange={(event) => updateRow(index, { value: event.target.value })}
                  />
                  <input
                    type="checkbox"
                    className="mx-auto size-3 accent-[var(--app-accent)]"
                    checked={row.sensitive}
                    onChange={(event) =>
                      updateRow(index, { sensitive: event.target.checked })
                    }
                  />
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-7 text-[var(--app-dim)] hover:text-destructive"
                    onClick={() => removeRow(index)}
                    title="Remove variable"
                  >
                    <Trash2 className="size-3.5" />
                  </Button>
                </div>
              ))
            ) : (
              <div className="px-3 py-8 text-center text-xs text-[var(--app-dim)]">
                No {"{{variables}}"} found in this request.
              </div>
            )}
          </div>
        </div>
      </ScrollArea>
    </div>
  );

  function updateRow(index: number, patch: Partial<RequestVariableRow>) {
    setRows((current) =>
      current.map((row, i) => (i === index ? { ...row, ...patch } : row)),
    );
    setSavedAt(undefined);
  }

  function removeRow(index: number) {
    const row = rows[index];
    if (!row) return;
    const scopeToRemove = row.originalScope ?? row.scope;
    setRemovedVariables((current) =>
      new Set(current).add(scopedVariableId(scopeToRemove, row.key)),
    );
    setRows((current) => current.filter((_, i) => i !== index));
    setSavedAt(undefined);
  }
}

function buildRows(
  keys: string[],
  globalVariables: VariableEntry[],
  collectionVariables: VariableEntry[],
  environmentVariables: VariableEntry[],
  activeEnvironmentId: EntityId | null | undefined,
): RequestVariableRow[] {
  const globals = new Map(globalVariables.map((variable) => [variable.key, variable]));
  const collection = new Map(
    collectionVariables.map((variable) => [variable.key, variable]),
  );
  const environment = new Map(
    environmentVariables.map((variable) => [variable.key, variable]),
  );

  return keys.map((key) => {
    const variable = environment.get(key) ?? collection.get(key) ?? globals.get(key);
    const scope = environment.has(key)
      ? "environment"
      : collection.has(key)
        ? "collection"
        : globals.has(key)
          ? "global"
          : activeEnvironmentId
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

function mergeVariables(
  existing: VariableEntry[],
  rows: RequestVariableRow[],
  scope: VariableScope,
  collectionId: EntityId | null,
  environmentId: EntityId | null,
  removedVariables: Set<string>,
): VariableEntry[] {
  const rowsByKey = new Map(rows.map((row) => [row.key, row]));
  const merged = existing
    .filter((variable) => {
      if (removedVariables.has(scopedVariableId(scope, variable.key))) return false;
      const row = rowsByKey.get(variable.key);
      return !row || row.originalScope !== scope || row.scope === scope;
    })
    .map((variable) => {
      const row = rowsByKey.get(variable.key);
      if (!row || row.scope !== scope) return variable;
      return rowToVariable(row, scope, collectionId, environmentId);
    });
  const existingKeys = new Set(merged.map((variable) => variable.key));

  for (const row of rows) {
    if (row.scope === scope && !existingKeys.has(row.key)) {
      merged.push(rowToVariable(row, scope, collectionId, environmentId));
    }
  }

  return merged;
}

function rowToVariable(
  row: RequestVariableRow,
  scope: VariableScope,
  collectionId: EntityId | null,
  environmentId: EntityId | null,
): VariableEntry {
  return {
    scope,
    collectionId: scope === "collection" ? collectionId : null,
    environmentId: scope === "environment" ? environmentId : null,
    key: row.key,
    value: row.value,
    initialValue: null,
    enabled: row.enabled,
    sensitive: row.sensitive,
    variableType: null,
  };
}

function scopedVariableId(scope: VariableScope, key: string) {
  return `${scope}:${key}`;
}

function extractRequestVariableNames(request: RequestDetail): string[] {
  const names = new Set<string>();
  const collect = (value?: string | null) => {
    if (!value) return;
    for (const match of value.matchAll(/\{\{\s*([^{}\s]+)\s*\}\}/g)) {
      names.add(match[1]);
    }
  };

  collect(request.name);
  collect(request.url);
  collectKeyValues(request.headers, collect);
  collectKeyValues(request.query, collect);
  collectKeyValues(request.pathParams, collect);
  collect(request.auth?.token);
  collect(request.auth?.username);
  collect(request.auth?.password);
  collect(request.auth?.key);
  collect(request.auth?.value);
  collect(request.auth?.addTo);

  if (request.body) {
    collect(request.body.raw);
    collect(request.body.rawLanguage);
    collectKeyValues(request.body.formData, collect);
    for (const field of request.body.formData) {
      collect(field.filePath);
      collect(field.contentType);
    }
    collectKeyValues(request.body.urlencoded, collect);
    collect(request.body.graphql?.query);
    collect(request.body.graphql?.variables);
    collect(request.body.file?.path);
    collect(request.body.file?.contentType);
  }

  collect(JSON.stringify(request.preRequestScript ?? null));
  collect(JSON.stringify(request.testScript ?? null));

  return [...names].sort((a, b) => a.localeCompare(b));
}

function collectKeyValues(
  rows: KeyValue[],
  collect: (value?: string | null) => void,
) {
  for (const row of rows) {
    collect(row.key);
    collect(row.value);
  }
}
