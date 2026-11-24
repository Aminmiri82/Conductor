import { memo, useEffect, useMemo, useRef, useState } from "react";
import {
  CellInput,
  CheckboxCell,
  HeaderRow,
  RemoveButton,
  Row,
} from "@/components/EditableTable";
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
import {
  createAutosaver,
  registerVariableAutosave,
  VARIABLE_AUTOSAVE,
} from "@/features/variables/autosave";
import { SaveState, type SaveStatus } from "@/features/variables/SaveState";
import { variableNames } from "@/features/variables/variableTokens";
import { useSaveVariables } from "@/features/variables/useSaveVariables";
import {
  applyRequestVariableChanges,
  requestVariableChanges,
  requestVariableRows,
  type RemovedVariable,
  type RequestVariableRow,
  type ScopedVariables,
} from "@/features/variables/variableChanges";
import type { KeyValue, VariableScope, VariableTarget } from "@/features/types";
import { useDraftValue } from "@/features/workspace/draftStore";
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

/**
 * What the table knows: the variables as stored, and the edits and removals
 * that are not saved yet. Replaced on every change. A ref holds the latest
 * one, so a save reads what was just typed, not what was last rendered.
 */
type Session = {
  stored: ScopedVariables;
  // By variable name.
  edits: ReadonlyMap<string, Partial<RequestVariableRow>>;
  removed: readonly RemovedVariable[];
};

// The rows for `keys`: what is stored, less the removals, with the unsaved
// edits on top.
function sessionRows(
  keys: readonly string[],
  { stored, edits, removed }: Session,
  hasEnvironment: boolean,
) {
  return requestVariableRows(
    keys,
    applyRequestVariableChanges(stored, [], removed),
    hasEnvironment,
  ).map((row) => ({ ...row, ...edits.get(row.key) }));
}

async function loadScopedVariables(
  collectionId: string,
  environmentId: string | null,
): Promise<ScopedVariables> {
  const [global, collection, environment] = await Promise.all([
    api.listVariables({ scope: "global" }),
    api.listVariables({ scope: "collection", collectionId }),
    environmentId
      ? api.listVariables({ scope: "environment", environmentId })
      : Promise.resolve([]),
  ]);
  return { global, collection, environment };
}

type VariableEditorProps = {
  requestId: string;
  collectionId: string;
  variableNames: string[];
};

// Memoized so typing in the URL re-renders this table only when the set of
// referenced names actually changes.
export const VariableEditor = memo(function VariableEditor(
  props: VariableEditorProps,
) {
  const environmentId = useWorkspaceUiStore((state) =>
    activeEnvironmentIdFor(state.workspaceUi, props.collectionId),
  );
  // Keyed so one table loads from and saves to one collection and
  // environment. Switching environment saves the pending edits to the one
  // they were made in, then starts a new table.
  return (
    <VariableTable
      key={`${props.collectionId}\n${environmentId ?? ""}`}
      environmentId={environmentId}
      {...props}
    />
  );
});

function VariableTable({
  requestId,
  collectionId,
  environmentId,
  variableNames,
}: VariableEditorProps & { environmentId: string | null }) {
  // Null until the stored variables have loaded.
  const [shown, setShown] = useState<Session | null>(null);
  const session = useRef<Session | null>(null);
  const [status, setStatus] = useState<SaveStatus>("idle");
  const { save: saveVariables } = useSaveVariables();
  const environmentName = useWorkspaceStore(
    (state) =>
      state.environments.find((environment) => environment.id === environmentId)
        ?.name,
  );
  const hasPathParams = useDraftValue(
    requestId,
    (request) => request.pathParams.length > 0,
  );
  const hasEnvironment = environmentId !== null;

  function commit(next: Session) {
    session.current = next;
    setShown(next);
  }

  async function persist() {
    const submitted = session.current;
    if (!submitted) return;
    if (!submitted.edits.size && !submitted.removed.length) return;
    const edited = sessionRows(
      [...submitted.edits.keys()],
      submitted,
      hasEnvironment,
    );
    const targetFor = (scope: VariableScope): VariableTarget | null => {
      if (scope === "global") return { scope };
      if (scope === "collection") return { scope, collectionId };
      return environmentId ? { scope, environmentId } : null;
    };

    setStatus("saving");
    const saved = await saveVariables(
      requestVariableChanges({
        edited,
        removed: submitted.removed,
        targetFor,
      }),
    );
    if (!saved) {
      setStatus("failed");
      return;
    }
    // What changed while the save ran stays pending.
    const latest = session.current ?? submitted;
    commit({
      stored: applyRequestVariableChanges(
        latest.stored,
        edited,
        submitted.removed,
      ),
      edits: new Map(
        [...latest.edits].filter(
          ([key, edit]) => submitted.edits.get(key) !== edit,
        ),
      ),
      removed: latest.removed.filter(
        (variable) => !submitted.removed.includes(variable),
      ),
    });
    setStatus("saved");
  }

  // Typing saves after a short pause; see `createAutosaver`. It keeps the
  // first render's `persist`, which stays right because `persist` reads only
  // the ref, state setters, and this table's collection and environment,
  // which never change (see the key above).
  const [autosaver] = useState(() =>
    createAutosaver(persist, VARIABLE_AUTOSAVE),
  );

  useEffect(() => {
    let mounted = true;
    loadScopedVariables(collectionId, environmentId).then(
      (stored) => {
        if (!mounted) return;
        const loaded: Session = { stored, edits: new Map(), removed: [] };
        session.current = loaded;
        setShown(loaded);
      },
      (error) => {
        if (mounted) useWorkspaceStore.getState().setError(error);
      },
    );
    return () => {
      mounted = false;
    };
  }, [collectionId, environmentId]);

  useEffect(() => {
    const unregister = registerVariableAutosave(() => autosaver.flush());
    // Leaving the table (another tab, request or environment) saves what is
    // still waiting. It stays registered until that save lands, so a send
    // right after still waits for it.
    return () => {
      void autosaver.flush().finally(unregister);
    };
  }, [autosaver]);

  const rows = useMemo(
    () => (shown ? sessionRows(variableNames, shown, hasEnvironment) : []),
    [shown, variableNames, hasEnvironment],
  );

  function edit(
    key: string,
    patch: Partial<RequestVariableRow>,
    when: "typing" | "now",
  ) {
    const current = session.current;
    if (!current) return;
    commit({
      ...current,
      edits: new Map(current.edits).set(key, {
        ...current.edits.get(key),
        ...patch,
      }),
    });
    if (when === "now") void autosaver.flush();
    else autosaver.schedule();
  }

  function remove(row: RequestVariableRow) {
    const current = session.current;
    if (!current) return;
    const edits = new Map(current.edits);
    edits.delete(row.key);
    commit({
      ...current,
      edits,
      removed: [
        ...current.removed,
        { scope: row.originalScope ?? row.scope, key: row.key },
      ],
    });
    void autosaver.flush();
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex min-h-12 items-center justify-between gap-3 border-b border-[var(--app-line)] px-4 py-2">
        <div className="min-w-0 text-xs text-[var(--app-dim)]">
          Every variable and path parameter this request uses. Variables save as
          you type; manage the full list in Settings.
          {environmentName ? ` Active environment: ${environmentName}.` : ""}
        </div>
        <div className="shrink-0">
          <SaveState status={status} onRetry={() => void autosaver.flush()} />
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
            <PathParamRows requestId={requestId} />
            {rows.map((row) => (
              <Row key={row.key} columns={columns}>
                <CheckboxCell
                  checked={row.enabled}
                  onChange={(enabled) => edit(row.key, { enabled }, "now")}
                />
                <div className="app-mono truncate px-2 text-xs text-[var(--app-text)]">
                  {row.key}
                </div>
                <Select
                  value={row.scope}
                  onValueChange={(value) => {
                    const scope = pickOption(scopeOptions, value);
                    if (scope) edit(row.key, { scope }, "now");
                  }}
                >
                  <SelectTrigger className="h-8 rounded-none border-0 bg-transparent px-2 text-xs shadow-none focus:ring-0">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {scopeOptions
                      .filter(([id]) => id !== "environment" || hasEnvironment)
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
                    edit(row.key, { value: event.target.value }, "typing")
                  }
                  onBlur={() => void autosaver.flush()}
                />
                <CheckboxCell
                  checked={row.sensitive}
                  onChange={(sensitive) => edit(row.key, { sensitive }, "now")}
                />
                {/* A name with no variable behind it has nothing to remove. */}
                {row.originalScope !== null || shown?.edits.has(row.key) ? (
                  <RemoveButton
                    className="hover:text-destructive"
                    onClick={() => remove(row)}
                    title="Remove variable"
                  />
                ) : (
                  <div />
                )}
              </Row>
            ))}
            {shown && !rows.length && !hasPathParams ? (
              <div className="px-3 py-8 text-center text-xs text-[var(--app-dim)]">
                No {"{{variables}}"} or :path parameters in this request.
              </div>
            ) : null}
          </div>
        </div>
      </ScrollArea>
    </div>
  );
}

// Path parameters belong to the request, not to a variable scope: these rows
// edit the draft, like the Params tab, and are saved with the request.
function PathParamRows({ requestId }: { requestId: string }) {
  const pathParams = useDraftValue(requestId, (request) => request.pathParams);
  const updateRequest = useWorkspaceStore((state) => state.updateRequest);
  if (!pathParams) return null;

  function update(index: number, patch: Partial<KeyValue>) {
    updateRequest({
      pathParams: pathParams?.map((param, i) =>
        i === index ? { ...param, ...patch } : param,
      ),
    });
  }

  return pathParams.map((param, index) => (
    <Row key={param.key} columns={columns}>
      <CheckboxCell
        checked={param.enabled ?? true}
        onChange={(enabled) => update(index, { enabled })}
      />
      <div className="app-mono truncate px-2 text-xs text-[var(--app-text)]">
        :{param.key}
      </div>
      <div className="px-2 text-xs text-[var(--app-dim)]">Path</div>
      <CellInput
        value={param.value}
        placeholder="Value"
        onChange={(event) => update(index, { value: event.target.value })}
      />
      <div />
      <div />
    </Row>
  ));
}

// The variable names a request references, sorted. Each part is scanned only
// when it changes, so a URL keystroke never rescans a large raw body, and the
// same array comes back while the set of names is unchanged.
export function useRequestVariableNames(requestId: string): string[] {
  const name = useDraftValue(requestId, (request) => request.name);
  const url = useDraftValue(requestId, (request) => request.url);
  const headers = useDraftValue(requestId, (request) => request.headers);
  const query = useDraftValue(requestId, (request) => request.query);
  const pathParams = useDraftValue(requestId, (request) => request.pathParams);
  const auth = useDraftValue(requestId, (request) => request.auth);
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
