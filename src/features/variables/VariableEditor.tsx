import {
  lazy,
  memo,
  Suspense,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Pencil, Save } from "lucide-react";
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
import { VariableOrigin } from "@/features/variables/VariableOrigin";
import {
  requestVariableChanges,
  type RemovedVariable,
  type RequestVariableRow,
} from "@/features/variables/variableChanges";
import type {
  KeyValue,
  VariableEntry,
  VariableScope,
  VariableTarget,
} from "@/features/types";
import { useDraftStore, useDraftValue } from "@/features/workspace/draftStore";
import {
  useActiveEnvironment,
  useWorkspaceStore,
} from "@/features/workspace/workspaceStore";

const SharedVariableDialog = lazy(() =>
  import("./SharedVariableDialog").then(({ SharedVariableDialog }) => ({
    default: SharedVariableDialog,
  })),
);

export default function VariablesSection({ requestId }: { requestId: string }) {
  const collectionId = useDraftValue(
    requestId,
    (request) => request.collectionId,
  );
  const linked = useDraftValue(requestId, (request) =>
    Boolean(request.repositoryRevision),
  );
  const variableNames = useRequestVariableNames(requestId);
  // Keyed so a request switch drops the previous request's unsaved edits.
  return collectionId !== undefined ? (
    <VariableEditor
      key={requestId}
      collectionId={collectionId}
      variableNames={variableNames}
      linked={Boolean(linked)}
    />
  ) : null;
}

const scopeOptions = [
  ["environment", "Environment"],
  ["collection", "Collection"],
  ["global", "Global"],
] as const satisfies readonly (readonly [VariableScope, string])[];

const columns =
  "grid-cols-[32px_minmax(105px,.8fr)_112px_minmax(130px,1.2fr)_66px_108px]";

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

// Memoized so typing in the URL re-renders this table only when the set of
// referenced names actually changes.
const VariableEditor = memo(function VariableEditor({
  collectionId,
  variableNames,
  linked,
}: {
  collectionId: string;
  variableNames: string[];
  linked: boolean;
}) {
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
  const [sharedEdit, setSharedEdit] = useState<{
    target: VariableTarget;
    key: string;
  }>();
  const { saving, save: saveVariables } = useSaveVariables();
  const activeEnvironment = useActiveEnvironment(collectionId);
  const activeEnvironmentId = activeEnvironment?.id;
  const activeEnvironmentName = activeEnvironment?.name;
  const collectionVersion = useWorkspaceStore((state) =>
    state.repositoryCollectionVersions.get(collectionId),
  );
  const lastSource = useRef("");
  const source = sourceKey(collectionId, activeEnvironmentId);
  const currentSource = useRef(source);
  currentSource.current = source;
  // Until the current environment's variables load, the rows are the previous
  // environment's; saving them then would write them to the new one.
  const ready = stored.source === source;

  useEffect(() => {
    let mounted = true;

    const onError = (error: unknown) => {
      if (mounted) useWorkspaceStore.getState().setError(error);
    };
    loadScopedVariables(collectionId, activeEnvironmentId).then((loaded) => {
      if (!mounted) return;
      setStored(loaded);
      if (lastSource.current !== loaded.source) {
        lastSource.current = loaded.source;
        setEdits(new Map());
        setRemovedVariables(new Map());
        setSavedAt(undefined);
      }
    }, onError);

    return () => {
      mounted = false;
    };
  }, [activeEnvironmentId, collectionId, collectionVersion]);

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

  function targetFor(scope: VariableScope): VariableTarget | null {
    if (scope === "global") return { scope };
    if (scope === "collection") return { scope, collectionId };
    return activeEnvironmentId
      ? { scope, environmentId: activeEnvironmentId }
      : null;
  }

  async function save() {
    if (!ready) return;
    const submittedEdits = edits;
    const submittedRemovals = removedVariables;
    const changes = requestVariableChanges({
      rows: linked
        ? rows.filter((row) => row.originalScope || edits.has(row.key))
        : rows,
      edited: edits,
      removed: removedVariables.values(),
      targetFor,
    });
    const originalScopes = new Map(
      rows.map((row) => [row.key, row.originalScope]),
    );
    const withBaselines = linked
      ? changes.map((change) => {
          const scope = change.target.scope;
          if (scope === "global") return change;
          const keys = new Set(change.upserts.map((row) => row.key));
          return {
            ...change,
            baseline: stored[scope].filter(
              (row) =>
                keys.has(row.key) && originalScopes.get(row.key) === scope,
            ),
          };
        })
      : changes;
    if (!(await saveVariables(withBaselines))) return;
    try {
      const reloaded = await loadScopedVariables(
        collectionId,
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
          {linked ? " Edits are saved in .conductor/local.json." : ""}
        </div>
        <div className="flex items-center gap-2">
          <span className="w-16 text-right text-xs text-[var(--app-dim)]">
            {saving ? "Saving" : savedAt ? "Saved" : ""}
          </span>
          <Button
            className="h-8 gap-1.5 bg-[var(--app-accent)] px-3 text-xs text-[var(--app-accent-fg)] hover:bg-[var(--app-accent)]/90"
            onClick={save}
            disabled={
              saving || !ready || (!rows.length && !removedVariables.size)
            }
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
                  {row.origin === "missingSecret" && !row.value ? (
                    <div />
                  ) : (
                    <CheckboxCell
                      checked={row.enabled}
                      onChange={(enabled) => updateRow(index, { enabled })}
                    />
                  )}
                  <div className="min-w-0 px-2">
                    <div className="app-mono truncate text-xs text-[var(--app-text)]">
                      {row.key}
                    </div>
                    <VariableOrigin origin={row.origin} />
                  </div>
                  <Select
                    value={row.scope}
                    disabled={row.presentation?.fixedKey}
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
                    aria-label={`Value for ${row.key}`}
                    type={row.sensitive ? "password" : "text"}
                    placeholder={
                      row.origin === "missingSecret"
                        ? "Enter required secret"
                        : undefined
                    }
                    onChange={(event) =>
                      updateRow(index, {
                        value: event.target.value,
                        enabled: row.origin === "missingSecret" || row.enabled,
                      })
                    }
                  />
                  {row.requiredSecret ? (
                    <span className="text-center text-[11px] text-[var(--app-dim)]">
                      Required
                    </span>
                  ) : (
                    <CheckboxCell
                      checked={row.sensitive}
                      onChange={(sensitive) => updateRow(index, { sensitive })}
                    />
                  )}
                  <div className="flex items-center justify-end gap-0.5 pr-1">
                    {row.presentation?.editShared ? (
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-7"
                        title="Edit shared value"
                        aria-label="Edit shared value"
                        onClick={() => {
                          const target =
                            row.originalScope && targetFor(row.originalScope);
                          if (target) setSharedEdit({ target, key: row.key });
                        }}
                        disabled={!ready || saving}
                      >
                        <Pencil className="size-3.5" />
                      </Button>
                    ) : null}
                    {row.presentation?.removal === "useShared" ? (
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-7 px-1.5 text-xs text-[var(--app-dim)]"
                        onClick={() => removeRow(index)}
                      >
                        Use shared value
                      </Button>
                    ) : row.presentation?.removal !== "none" &&
                      (row.origin !== "missingSecret" || row.value) ? (
                      <RemoveButton
                        className="hover:text-destructive"
                        onClick={() => removeRow(index)}
                        title={
                          row.presentation
                            ? "Remove local value"
                            : "Remove variable"
                        }
                      />
                    ) : null}
                  </div>
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
      {sharedEdit ? (
        <Suspense>
          <SharedVariableDialog
            target={sharedEdit.target}
            variableKey={sharedEdit.key}
            onClose={() => setSharedEdit(undefined)}
          />
        </Suspense>
      ) : null}
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
});

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
      ...variable,
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

// The variable names a request references, sorted. Each part is scanned only
// when it changes, so a URL keystroke never rescans a large raw body, and the
// same array comes back while the set of names is unchanged.
function useRequestVariableNames(requestId: string): string[] {
  const name = useDraftValue(requestId, (request) => request.name);
  const url = useDraftValue(requestId, (request) => request.url);
  const headers = useDraftValue(requestId, (request) => request.headers);
  const query = useDraftValue(requestId, (request) => request.query);
  const pathParams = useDraftValue(requestId, (request) => request.pathParams);
  const ownAuth = useDraftValue(requestId, (request) => request.auth);
  const inheritedAuth = useDraftStore(
    (state) => state.previews.get(requestId)?.inheritedAuth,
  );
  const auth = ownAuth ?? inheritedAuth;
  const body = useDraftValue(requestId, (request) => request.body);
  const preRequestScript = useDraftValue(
    requestId,
    (request) => request.preRequestScript,
  );
  const testScript = useDraftValue(requestId, (request) => request.testScript);

  const lineNames = useMemo(
    () =>
      collectNames((collect) => {
        collect(name);
        collect(url);
        collectKeyValues(headers ?? [], collect);
        collectKeyValues(query ?? [], collect);
        collectKeyValues(pathParams ?? [], collect);
        collect(auth?.token);
        collect(auth?.username);
        collect(auth?.password);
        collect(auth?.key);
        collect(auth?.value);
      }),
    [name, url, headers, query, pathParams, auth],
  );
  const bodyNames = useMemo(
    () =>
      collectNames((collect) => {
        if (!body) return;
        collect(body.raw);
        collectKeyValues(body.formData ?? [], collect);
        for (const field of body.formData ?? []) {
          collect(field.filePath);
          collect(field.contentType);
        }
        collectKeyValues(body.urlencoded ?? [], collect);
        collect(body.graphql?.query);
        collect(body.graphql?.variables);
        collect(body.file?.path);
        collect(body.file?.contentType);
      }),
    [body],
  );
  const scriptNames = useMemo(
    () =>
      collectNames((collect) => {
        collect(JSON.stringify(preRequestScript ?? null));
        collect(JSON.stringify(testScript ?? null));
      }),
    [preRequestScript, testScript],
  );

  // Joined into one string so an unchanged set keeps the same array.
  const joined = useMemo(
    () =>
      [...new Set([...lineNames, ...bodyNames, ...scriptNames])]
        .sort((a, b) => a.localeCompare(b))
        .join("\0"),
    [lineNames, bodyNames, scriptNames],
  );
  return useMemo(() => (joined ? joined.split("\0") : []), [joined]);
}

function collectNames(
  scan: (collect: (value?: string | null) => void) => void,
): Set<string> {
  const names = new Set<string>();
  scan((value) => {
    if (!value) return;
    // `{{}}` has no name to edit; Rust lists it as unresolved.
    for (const name of variableNames(value)) {
      if (name) names.add(name);
    }
  });
  return names;
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
