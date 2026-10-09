import { useEffect, useMemo, useRef, useState } from "react";
import { Save } from "lucide-react";
import {
  CellInput,
  CheckboxCell,
  HeaderRow,
  RemoveButton,
  Row,
} from "@/components/EditableTable";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { api } from "@/lib/tauri";
import { pickOption } from "@/lib/options";
import { variableNames } from "@/features/variables/variableTokens";
import { useSaveVariables } from "@/features/variables/useSaveVariables";
import {
  requestVariableChanges,
  type RemovedVariable,
  type RequestVariableRow,
} from "@/features/variables/variableChanges";
import type {
  KeyValue,
  RequestDetail,
  VariableEntry,
  VariableScope,
} from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import {
  activeEnvironmentIdFor,
  useWorkspaceUiStore,
} from "@/features/workspace/workspaceUiStore";

const scopeOptions = [
  ["environment", "Environment"],
  ["collection", "Collection"],
  ["global", "Global"],
] as const satisfies readonly (readonly [VariableScope, string])[];

const columns =
  "grid-cols-[32px_minmax(130px,.8fr)_112px_minmax(170px,1.2fr)_80px_44px]";

type ScopedVariables = {
  // Which collection and environment these were listed for.
  source: string;
  global: VariableEntry[];
  collection: VariableEntry[];
  environment: VariableEntry[];
};

function sourceKey(collectionId: string, environmentId?: string | null) {
  return `${collectionId}\n${environmentId ?? ""}`;
}

async function loadScopedVariables(
  collectionId: string,
  environmentId: string | null | undefined,
): Promise<ScopedVariables> {
  const [global, collection, environment] = await Promise.all([
    api.listVariables({ scope: "global" }),
    api.listVariables({ scope: "collection", collectionId }),
    environmentId
      ? api.listVariables({ scope: "environment", environmentId })
      : Promise.resolve([]),
  ]);
  return {
    source: sourceKey(collectionId, environmentId),
    global,
    collection,
    environment,
  };
}

export function VariableEditor({ request }: { request: RequestDetail }) {
  const { name, url, headers, query, pathParams, auth, body } = request;
  const { preRequestScript, testScript } = request;
  const variableNames = useMemo(
    () =>
      extractRequestVariableNames({
        name,
        url,
        headers,
        query,
        pathParams,
        auth,
        body,
        preRequestScript,
        testScript,
      }),
    [
      name,
      url,
      headers,
      query,
      pathParams,
      auth,
      body,
      preRequestScript,
      testScript,
    ],
  );
  const [stored, setStored] = useState<ScopedVariables>({
    source: "",
    global: [],
    collection: [],
    environment: [],
  });
  // Unsaved edits are kept per key, apart from the stored variables, so
  // editing the request (which changes which names show) never discards them.
  const [edits, setEdits] = useState<
    ReadonlyMap<string, Partial<RequestVariableRow>>
  >(new Map());
  const [removedVariables, setRemovedVariables] = useState<
    ReadonlyMap<string, RemovedVariable>
  >(new Map());
  const [savedAt, setSavedAt] = useState<number>();
  const { saving, save: saveVariables } = useSaveVariables();
  const activeEnvironmentId = useWorkspaceUiStore((state) =>
    activeEnvironmentIdFor(state.workspaceUi, request.collectionId),
  );
  const environments = useWorkspaceStore((state) => state.environments);
  const activeEnvironmentName = environments.find(
    (environment) => environment.id === activeEnvironmentId,
  )?.name;
  const source = sourceKey(request.collectionId, activeEnvironmentId);
  const currentSource = useRef(source);
  currentSource.current = source;
  // Until the current environment's variables load, the rows are the previous
  // environment's; saving them then would write them to the new one.
  const ready = stored.source === source;

  useEffect(() => {
    let mounted = true;

    loadScopedVariables(request.collectionId, activeEnvironmentId).then(
      (loaded) => {
        if (!mounted) return;
        setStored(loaded);
        setEdits(new Map());
        setRemovedVariables(new Map());
        setSavedAt(undefined);
      },
      (error) => {
        if (mounted) useWorkspaceStore.getState().setError(error);
      },
    );

    return () => {
      mounted = false;
    };
  }, [activeEnvironmentId, request.collectionId]);

  const rows = useMemo(
    () =>
      buildRows(
        variableNames,
        stored.global,
        stored.collection,
        stored.environment,
        activeEnvironmentId,
      )
        .map((row) => ({ ...row, ...edits.get(row.key) }))
        .filter(
          (row) =>
            !removedVariables.has(
              scopedVariableId(row.originalScope ?? row.scope, row.key),
            ),
        ),
    [activeEnvironmentId, edits, removedVariables, stored, variableNames],
  );

  async function save() {
    if (!ready) return;
    const submittedEdits = edits;
    const submittedRemovals = removedVariables;
    const changes = requestVariableChanges({
      rows,
      edited: edits,
      removed: removedVariables.values(),
      targetFor: (scope) => {
        if (scope === "global") return { scope };
        if (scope === "collection") {
          return { scope, collectionId: request.collectionId };
        }
        return activeEnvironmentId
          ? { scope, environmentId: activeEnvironmentId }
          : null;
      },
    });
    if (!(await saveVariables(changes))) return;
    try {
      const reloaded = await loadScopedVariables(
        request.collectionId,
        activeEnvironmentId,
      );
      if (reloaded.source !== currentSource.current) return;
      setStored(reloaded);
      // Edits typed while the save ran stay pending.
      setEdits((current) => (current === submittedEdits ? new Map() : current));
      setRemovedVariables((current) =>
        current === submittedRemovals ? new Map() : current,
      );
      setSavedAt(Date.now());
    } catch (error) {
      useWorkspaceStore.getState().setError(error);
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex min-h-12 items-center justify-between gap-3 border-b border-[var(--app-line)] px-4 py-2">
        <div className="min-w-0 text-xs text-[var(--app-dim)]">
          Shows variables referenced in this request. Manage the full list in
          Settings.
          {activeEnvironmentName
            ? ` Active environment: ${activeEnvironmentName}.`
            : ""}
        </div>
        <div className="flex items-center gap-2">
          <span className="w-16 text-right text-xs text-[var(--app-dim)]">
            {saving ? "Saving" : savedAt ? "Saved" : ""}
          </span>
          <Button
            className="h-8 gap-1.5 bg-[var(--app-accent)] px-3 text-xs text-[var(--app-accent-fg)] hover:bg-[var(--app-accent)]/90"
            onClick={save}
            disabled={saving || !ready || rows.length === 0}
          >
            <Save className="size-3.5" />
            Save
          </Button>
        </div>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="p-4">
          <div className="overflow-hidden border border-[var(--app-line)] bg-[var(--app-panel)]">
            <HeaderRow columns={columns} className="h-9">
              <div />
              <div>Key</div>
              <div>Scope</div>
              <div>Value</div>
              <div>Secret</div>
              <div />
            </HeaderRow>
            {rows.length ? (
              rows.map((row, index) => (
                <Row key={row.key} columns={columns}>
                  <CheckboxCell
                    checked={row.enabled}
                    onChange={(enabled) => updateRow(index, { enabled })}
                  />
                  <div className="app-mono truncate px-2 text-xs text-[var(--app-text)]">
                    {row.key}
                  </div>
                  <Select
                    value={row.scope}
                    onValueChange={(value) => {
                      const scope = pickOption(scopeOptions, value);
                      if (scope) updateRow(index, { scope });
                    }}
                  >
                    <SelectTrigger className="h-8 rounded-none border-0 bg-transparent px-2 text-xs shadow-none focus:ring-0">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {scopeOptions
                        .filter(
                          ([id]) => id !== "environment" || activeEnvironmentId,
                        )
                        .map(([id, label]) => (
                          <SelectItem key={id} value={id}>
                            {label}
                          </SelectItem>
                        ))}
                    </SelectContent>
                  </Select>
                  <CellInput
                    value={row.value}
                    type={row.sensitive ? "password" : "text"}
                    onChange={(event) =>
                      updateRow(index, { value: event.target.value })
                    }
                  />
                  <CheckboxCell
                    checked={row.sensitive}
                    onChange={(sensitive) => updateRow(index, { sensitive })}
                  />
                  <RemoveButton
                    className="hover:text-destructive"
                    onClick={() => removeRow(index)}
                    title="Remove variable"
                  />
                </Row>
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
    const key = rows[index]?.key;
    if (key === undefined) return;
    setEdits((current) =>
      new Map(current).set(key, { ...current.get(key), ...patch }),
    );
    setSavedAt(undefined);
  }

  function removeRow(index: number) {
    const row = rows[index];
    if (!row) return;
    const scopeToRemove = row.originalScope ?? row.scope;
    setRemovedVariables((current) =>
      new Map(current).set(scopedVariableId(scopeToRemove, row.key), {
        scope: scopeToRemove,
        key: row.key,
      }),
    );
    setSavedAt(undefined);
  }
}

function buildRows(
  keys: string[],
  globalVariables: VariableEntry[],
  collectionVariables: VariableEntry[],
  environmentVariables: VariableEntry[],
  activeEnvironmentId: string | null | undefined,
): RequestVariableRow[] {
  const globals = new Map(
    globalVariables.map((variable) => [variable.key, variable]),
  );
  const collection = new Map(
    collectionVariables.map((variable) => [variable.key, variable]),
  );
  const environment = new Map(
    environmentVariables.map((variable) => [variable.key, variable]),
  );

  return keys.map((key) => {
    const variable =
      environment.get(key) ?? collection.get(key) ?? globals.get(key);
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

function scopedVariableId(scope: VariableScope, key: string) {
  return `${scope}:${key}`;
}

function extractRequestVariableNames(
  request: Pick<
    RequestDetail,
    | "name"
    | "url"
    | "headers"
    | "query"
    | "pathParams"
    | "auth"
    | "body"
    | "preRequestScript"
    | "testScript"
  >,
): string[] {
  const names = new Set<string>();
  const collect = (value?: string | null) => {
    if (!value) return;
    // `{{}}` has no name to edit; Rust lists it as unresolved.
    for (const name of variableNames(value)) {
      if (name) names.add(name);
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

  if (request.body) {
    collect(request.body.raw);
    collectKeyValues(request.body.formData ?? [], collect);
    for (const field of request.body.formData ?? []) {
      collect(field.filePath);
      collect(field.contentType);
    }
    collectKeyValues(request.body.urlencoded ?? [], collect);
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
