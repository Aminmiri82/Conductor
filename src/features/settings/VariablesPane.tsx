import { confirm } from "@tauri-apps/plugin-dialog";
import {
  type ChangeEvent,
  type ReactNode,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  Copy,
  Ellipsis,
  Eye,
  EyeOff,
  Lock,
  LockOpen,
  Pencil,
  Plus,
  Power,
  Trash2,
  Upload,
} from "lucide-react";
import { CellInput, Row, TableFrame } from "@/components/EditableTable";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { api } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import type {
  VariableEntry,
  VariableScope,
  VariableTarget,
} from "@/features/types";
import { createAutosaver } from "@/features/variables/autosave";
import {
  diffVariables,
  duplicateKeys,
  normalizeVariables,
} from "@/features/variables/variableChanges";
import { useSaveVariables } from "@/features/variables/useSaveVariables";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import {
  activeEnvironmentIdFor,
  useWorkspaceUiStore,
} from "@/features/workspace/workspaceUiStore";
import {
  drawerAction,
  EditableTitle,
  ListDrawer,
  Segmented,
  SectionTitle,
} from "./SettingsParts";

const columns = "grid-cols-[minmax(140px,.8fr)_minmax(180px,1.4fr)_120px]";

// Row actions stay out of the way until the row is hovered or focused.
const onRowHover =
  "opacity-0 group-focus-within:opacity-100 group-hover:opacity-100";

const pickerClass =
  "h-8 w-44 border-[var(--app-line)] bg-[var(--app-panel-2)] text-xs";

const outlineButton =
  "h-8 gap-1.5 border border-[var(--app-line)] px-2.5 text-xs text-[var(--app-dim)]";

// Typing saves after a short pause; see `createAutosaver`.
const AUTOSAVE_DELAY_MS = 400;
const AUTOSAVE_MAX_WAIT_MS = 2000;

/**
 * The table being edited: what is stored for its target (`saved`) and what
 * the table shows (`rows`). Kept in a ref so a save started for one target
 * never writes another target's rows.
 */
type Session = {
  target: VariableTarget;
  saved: VariableEntry[];
  rows: VariableEntry[];
};

type Renaming = { environmentId: string; at: "title" | "drawer" };

type SaveStatus = "idle" | "saving" | "saved" | "failed";

export function VariablesPane({
  activeCollectionId,
}: {
  activeCollectionId?: string;
}) {
  const collections = useWorkspaceStore((state) => state.collections);
  const allEnvironments = useWorkspaceStore((state) => state.environments);
  const selectEnvironment = useWorkspaceStore(
    (state) => state.selectEnvironment,
  );
  const loadEnvironments = useWorkspaceStore((state) => state.loadEnvironments);
  const importEnvironment = useWorkspaceStore(
    (state) => state.importEnvironment,
  );
  const [scope, setScope] = useState<VariableScope>(
    activeCollectionId || collections[0] ? "environment" : "global",
  );
  const [pickedCollectionId, setPickedCollectionId] =
    useState(activeCollectionId);
  const selectedCollectionId = pickedCollectionId ?? collections[0]?.id ?? "";
  const activeEnvironmentId = useWorkspaceUiStore((state) =>
    activeEnvironmentIdFor(state.workspaceUi, selectedCollectionId),
  );
  const environments = allEnvironments.filter(
    (environment) => environment.collectionId === selectedCollectionId,
  );
  const [pickedEnvironmentId, setPickedEnvironmentId] = useState("");
  // Viewing an environment here does not make it active. When the pick is
  // gone (another collection, or deleted), show the collection's active
  // environment, or its first one.
  const selectedEnvironment =
    environments.find(
      (environment) => environment.id === pickedEnvironmentId,
    ) ??
    environments.find(
      (environment) => environment.id === activeEnvironmentId,
    ) ??
    environments[0];
  const selectedEnvironmentId = selectedEnvironment?.id ?? "";
  const [variables, setVariables] = useState<VariableEntry[]>([]);
  const [status, setStatus] = useState<SaveStatus>("idle");
  const { save: saveVariables } = useSaveVariables();
  const session = useRef<Session | null>(null);
  // The row whose key field takes focus after the next render: a row just
  // added or duplicated, or the one that takes a deleted row's place.
  const focusRow = useRef<number | null>(null);
  const addAfterMenu = useRef(false);
  // The environment being renamed, in the heading or in its drawer row.
  const [renaming, setRenaming] = useState<Renaming | null>(null);
  const [revealed, setRevealed] = useState<Record<number, boolean>>({});
  const environmentInputRef = useRef<HTMLInputElement>(null);

  async function persist(current: Session) {
    // Rust rejects a table with a key twice; the rows show the clash and the
    // save waits until it is resolved.
    if (duplicateKeys(current.rows).size) return;
    const changes = diffVariables(current.saved, current.rows);
    if (!changes.upserts.length && !changes.deletes.length) return;
    const sent = normalizeVariables(current.rows);
    const isShown = () => session.current === current;
    if (isShown()) setStatus("saving");
    const saved = await saveVariables([{ target: current.target, ...changes }]);
    if (saved) current.saved = sent;
    if (isShown()) setStatus(saved ? "saved" : "failed");
  }

  const [autosaver] = useState(() =>
    createAutosaver(
      async () => {
        if (session.current) await persist(session.current);
      },
      { delayMs: AUTOSAVE_DELAY_MS, maxWaitMs: AUTOSAVE_MAX_WAIT_MS },
    ),
  );

  // Null until the scope has the collection or environment it needs.
  const target = useMemo((): VariableTarget | null => {
    if (scope === "global") return { scope };
    if (scope === "collection") {
      return selectedCollectionId
        ? { scope, collectionId: selectedCollectionId }
        : null;
    }
    return selectedEnvironmentId
      ? { scope, environmentId: selectedEnvironmentId }
      : null;
  }, [scope, selectedCollectionId, selectedEnvironmentId]);

  useEffect(() => {
    let mounted = true;
    session.current = null;
    setStatus("idle");
    const show = (rows: VariableEntry[]) => {
      if (target) session.current = { target, saved: rows, rows };
      setVariables(rows);
      setRevealed({});
    };
    if (!target) {
      show([]);
    } else {
      api.listVariables(target).then(
        (items) => {
          if (mounted) show(items);
        },
        (error) => {
          if (!mounted) return;
          setVariables([]);
          useWorkspaceStore.getState().setError(error);
        },
      );
    }
    return () => {
      mounted = false;
      // Leaving this target (or the pane): save what is still waiting.
      autosaver.cancel();
      const leaving = session.current;
      if (leaving?.target === target) void persist(leaving);
    };
    // persist only reads refs and stable setters.
    // oxlint-disable-next-line react/exhaustive-deps
  }, [target, autosaver]);

  function edit(rows: VariableEntry[], when: "typing" | "now") {
    setVariables(rows);
    if (!session.current) return;
    session.current.rows = rows;
    if (when === "now") void autosaver.flush();
    else autosaver.schedule();
  }

  function updateVariable(
    index: number,
    patch: Partial<VariableEntry>,
    when: "typing" | "now",
  ) {
    const rows = session.current?.rows ?? variables;
    edit(
      rows.map((variable, i) =>
        i === index ? { ...variable, ...patch } : variable,
      ),
      when,
    );
  }

  // Reveal state is kept by row index, so it moves with rows that shift.
  function shiftRevealed(from: number, by: 1 | -1) {
    setRevealed((current) =>
      Object.fromEntries(
        Object.entries(current)
          .filter(([key]) => by === 1 || Number(key) !== from)
          .map(([key, value]) => [
            Number(key) >= from && !(by === -1 && Number(key) === from)
              ? String(Number(key) + by)
              : key,
            value,
          ]),
      ),
    );
  }

  function removeVariable(index: number, refocus = false) {
    const rows = session.current?.rows ?? variables;
    if (refocus && rows.length > 1) {
      focusRow.current = Math.min(index, rows.length - 2);
    }
    edit(
      rows.filter((_, i) => i !== index),
      "now",
    );
    shiftRevealed(index, -1);
  }

  // The copy keeps the key, so it shows as a clash until it is renamed; its
  // key is selected for that.
  function duplicateVariable(index: number) {
    const rows = session.current?.rows ?? variables;
    const copy = rows[index];
    if (!copy) return;
    focusRow.current = index + 1;
    edit(
      [...rows.slice(0, index + 1), { ...copy }, ...rows.slice(index + 1)],
      "typing",
    );
    shiftRevealed(index + 1, 1);
  }

  function addVariable() {
    if (!target) return;
    const rows = session.current?.rows ?? variables;
    focusRow.current = rows.length;
    edit(
      [...rows, { key: "", value: "", enabled: true, sensitive: false }],
      "typing",
    );
  }

  useEffect(() => {
    const index = focusRow.current;
    if (index === null) return;
    focusRow.current = null;
    const input = document.querySelector<HTMLInputElement>(
      `input[data-variable-index="${index}"]`,
    );
    input?.focus();
    input?.select();
  });

  // While this pane is open, ⌘N, ⌘⇧N and ⌘D act on variables instead of
  // requests (see hotkeys.ts), and ⌘I and ⌘⌫ are handled here. Read through a
  // ref so the listeners see the current table.
  const shortcuts = {
    new: addVariable,
    newEnvironment: () => {
      if (scope === "environment") void createEnvironment();
    },
    duplicateEnvironment: () => {
      if (scope === "environment" && selectedEnvironmentId) {
        void duplicateSelectedEnvironment();
      }
    },
    duplicate: () => {
      const index = focusedVariableIndex();
      if (index !== undefined) duplicateVariable(index);
    },
    remove: () => {
      const index = focusedVariableIndex();
      if (index !== undefined) removeVariable(index, true);
    },
    import: () => {
      if (scope === "environment" && selectedCollectionId) {
        environmentInputRef.current?.click();
      }
    },
  };
  const shortcutsRef = useRef(shortcuts);
  useEffect(() => {
    shortcutsRef.current = shortcuts;
  });
  useEffect(() => {
    const claims = [
      ["conductor:new", "new"],
      ["conductor:new-folder", "newEnvironment"],
      ["conductor:duplicate", "duplicate"],
    ] as const;
    const listeners = claims.map(([name, action]) => {
      const listener = (event: Event) => {
        event.preventDefault();
        shortcutsRef.current[action]();
      };
      window.addEventListener(name, listener);
      return () => window.removeEventListener(name, listener);
    });
    function onKeyDown(event: KeyboardEvent) {
      if (!(event.metaKey || event.ctrlKey)) return;
      const key = event.key.toLowerCase();
      // ⌘⇧D is not a menu shortcut, so it arrives here rather than through
      // hotkeys.ts.
      if (event.shiftKey) {
        if (key === "d") {
          event.preventDefault();
          shortcutsRef.current.duplicateEnvironment();
        }
        return;
      }
      if (key === "i") {
        event.preventDefault();
        shortcutsRef.current.import();
      }
      // In a variable's field this replaces "delete to line start".
      if (
        (key === "backspace" || key === "delete") &&
        focusedVariableIndex() !== undefined
      ) {
        event.preventDefault();
        shortcutsRef.current.remove();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => {
      listeners.forEach((dispose) => dispose());
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  function chooseEnvironment(environmentId: string) {
    setRenaming(null);
    setPickedEnvironmentId(environmentId);
  }

  function startRename(environmentId: string, at: Renaming["at"]) {
    setRenaming({ environmentId, at });
  }

  async function finishRename(environmentId: string, draft: string | null) {
    // A field that closes because another rename started (a new environment
    // made while this one was being named) must not end that one.
    setRenaming((current) =>
      current?.environmentId === environmentId ? null : current,
    );
    const name = draft?.trim();
    const current = environments.find((e) => e.id === environmentId)?.name;
    if (!name || name === current) return;
    try {
      await api.renameEnvironment(environmentId, name);
      await loadEnvironments();
    } catch (error) {
      useWorkspaceStore.getState().setError(error);
    }
  }

  async function createEnvironment() {
    if (!selectedCollectionId) return;
    try {
      const environmentId = await api.createEnvironment(
        selectedCollectionId,
        "New environment",
      );
      await loadEnvironments();
      chooseEnvironment(environmentId);
      startRename(environmentId, "title");
    } catch (error) {
      useWorkspaceStore.getState().setError(error);
    }
  }

  async function duplicateSelectedEnvironment() {
    if (!selectedEnvironmentId) return;
    try {
      // The copy must include edits still waiting to save.
      await autosaver.flush();
      const environmentId = await api.duplicateEnvironment(
        selectedEnvironmentId,
      );
      await loadEnvironments();
      chooseEnvironment(environmentId);
      startRename(environmentId, "title");
    } catch (error) {
      useWorkspaceStore.getState().setError(error);
    }
  }

  async function onEnvironmentFileChange(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file || !selectedCollectionId) return;
    const text = await file.text();
    await importEnvironment(selectedCollectionId, text, file.name);
    setScope("environment");
    event.target.value = "";
  }

  // The selection falls back on its own when the selected one goes; see
  // `selectedEnvironment`.
  async function deleteEnvironment(deletedId: string) {
    const name = environments.find((e) => e.id === deletedId)?.name;
    const confirmed = await confirm(`Delete environment "${name}"?`, {
      kind: "warning",
    });
    if (!confirmed) return;
    try {
      await api.deleteEnvironment(deletedId);
      await loadEnvironments();
      // Deleting the active environment leaves the collection with none
      // active rather than activating another one.
      if (deletedId === activeEnvironmentId) {
        await selectEnvironment(selectedCollectionId, null);
      }
    } catch (error) {
      useWorkspaceStore.getState().setError(error);
    }
  }

  const duplicates = duplicateKeys(variables);
  const selectedCollectionName = collections.find(
    (collection) => collection.id === selectedCollectionId,
  )?.name;

  const newVariableButton = (
    <Button
      variant="ghost"
      size="icon"
      className="size-7 shrink-0 border border-[var(--app-line)] text-[var(--app-dim)]"
      title="New variable (⌘N)"
      aria-label="New variable"
      onClick={addVariable}
    >
      <Plus className="size-3.5" />
    </Button>
  );

  const table = target ? (
    <TableFrame className="bg-[var(--app-panel)]">
      <div
        // Leaving the table saves at once rather than after the pause.
        onBlur={() => void autosaver.flush()}
      >
        {variables.map((variable, index) => {
          const duplicate = duplicates.has(variable.key.trim());
          return (
            <Row
              key={index}
              columns={columns}
              className="group relative hover:bg-[var(--app-panel-2)]"
            >
              <CellInput
                data-variable-index={index}
                className={cn(
                  !variable.enabled && "text-[var(--app-dim)] line-through",
                  duplicate && "text-destructive",
                )}
                value={variable.key}
                placeholder="Key"
                title={duplicate ? "This key is used twice" : undefined}
                onChange={(event) =>
                  updateVariable(index, { key: event.target.value }, "typing")
                }
              />
              <CellInput
                data-variable-index={index}
                className={cn(
                  !variable.enabled && "text-[var(--app-dim)] line-through",
                )}
                type={
                  variable.sensitive && !revealed[index] ? "password" : "text"
                }
                value={variable.value}
                placeholder="Value"
                onChange={(event) =>
                  updateVariable(index, { value: event.target.value }, "typing")
                }
              />
              <div className="flex items-center justify-end pr-1">
                {variable.enabled ? null : (
                  // Imports can bring disabled rows; this is the only
                  // way back, so it stays visible.
                  <RowAction
                    className="text-[var(--app-accent)] hover:text-[var(--app-accent)]"
                    onClick={() =>
                      updateVariable(index, { enabled: true }, "now")
                    }
                    title="Disabled. Click to enable"
                  >
                    <Power />
                  </RowAction>
                )}
                {variable.sensitive ? (
                  <RowAction
                    className={onRowHover}
                    onClick={() =>
                      setRevealed((current) => ({
                        ...current,
                        [index]: !current[index],
                      }))
                    }
                    title={revealed[index] ? "Hide value" : "Reveal value"}
                  >
                    {revealed[index] ? <EyeOff /> : <Eye />}
                  </RowAction>
                ) : null}
                <RowAction
                  className={cn(
                    variable.sensitive
                      ? "text-[var(--app-accent)] hover:text-[var(--app-accent)]"
                      : onRowHover,
                    variable.sensitive && !variable.enabled && "opacity-50",
                  )}
                  onClick={() =>
                    updateVariable(
                      index,
                      { sensitive: !variable.sensitive },
                      "now",
                    )
                  }
                  title={
                    variable.sensitive
                      ? "Secret. Click to unmark"
                      : "Mark as secret"
                  }
                >
                  {variable.sensitive ? <Lock /> : <LockOpen />}
                </RowAction>
                <RowAction
                  className={cn(onRowHover, "hover:text-destructive")}
                  onClick={() => removeVariable(index)}
                  title="Remove variable (⌘⌫)"
                >
                  <Trash2 />
                </RowAction>
              </div>
            </Row>
          );
        })}
      </div>
      {variables.length ? null : (
        <div className="flex h-9 items-center px-3 text-xs text-[var(--app-dim)]">
          No variables yet. Press <Kbd>⌘N</Kbd> to add one.
        </div>
      )}
    </TableFrame>
  ) : null;

  return (
    <div>
      <SectionTitle
        title="Variables"
        sub="Each collection has its own environments. Environment variables override collection variables, which override globals."
      />
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <Segmented
          value={scope}
          options={[
            ["environment", "Environment"],
            ["collection", "Collection"],
            ["global", "Global"],
          ]}
          onChange={setScope}
        />
        {/* Collection scope picks its collection from the list below. */}
        {scope === "environment" && collections.length ? (
          <Select
            value={selectedCollectionId}
            onValueChange={setPickedCollectionId}
          >
            <SelectTrigger className={pickerClass} aria-label="Collection">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {collections.map((collection) => (
                <SelectItem key={collection.id} value={collection.id}>
                  {collection.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : null}
        <div className="ml-auto flex shrink-0 items-center gap-3">
          <SaveState
            status={status}
            duplicate={[...duplicates][0]}
            onRetry={() => void autosaver.flush()}
          />
          <input
            ref={environmentInputRef}
            type="file"
            accept="application/json,.json,.env,text/plain"
            className="hidden"
            onChange={(event) => void onEnvironmentFileChange(event)}
          />
        </div>
      </div>
      {scope === "global" ? (
        <>
          <div className="mb-3 -ml-2 flex min-h-8 items-center gap-2.5">
            <EditableTitle name="Globals" label="Globals" />
            {newVariableButton}
          </div>
          {table}
        </>
      ) : null}
      {scope !== "global" && !collections.length ? (
        <div className="text-xs text-[var(--app-dim)]">
          Create or import a collection to add collection and environment
          variables.
        </div>
      ) : null}
      {scope === "environment" && collections.length ? (
        environments.length ? (
          <div className="grid grid-cols-[184px_minmax(0,1fr)] gap-4">
            <ListDrawer
              title="Environments"
              items={environments.map((environment) => ({
                id: environment.id,
                name: environment.name,
                markTitle:
                  environment.id === activeEnvironmentId
                    ? "Active for sending"
                    : undefined,
              }))}
              selectedId={selectedEnvironmentId}
              onSelect={chooseEnvironment}
              itemActions={[
                {
                  label: "Rename",
                  icon: <Pencil />,
                  run: (id) => startRename(id, "drawer"),
                },
                {
                  label: "Delete",
                  icon: <Trash2 />,
                  destructive: true,
                  run: (id) => void deleteEnvironment(id),
                },
              ]}
              editingId={
                renaming?.at === "drawer" ? renaming.environmentId : undefined
              }
              editLabel="Environment name"
              onFinishEdit={(id, name) => void finishRename(id, name)}
              createTitle="New environment"
              onCreate={() => void createEnvironment()}
              actions={
                <Button
                  variant="ghost"
                  size="icon"
                  className={drawerAction}
                  title="Import a Postman environment or .env file (⌘I)"
                  aria-label="Import environment"
                  onClick={() => environmentInputRef.current?.click()}
                >
                  <Upload className="size-3.5" />
                </Button>
              }
            />
            {selectedEnvironment ? (
              <div className="min-w-0">
                <div className="mb-3 -ml-2 flex min-h-8 items-center gap-2.5">
                  <EditableTitle
                    key={selectedEnvironment.id}
                    name={selectedEnvironment.name}
                    label="Environment name"
                    editing={
                      renaming?.at === "title" &&
                      renaming.environmentId === selectedEnvironment.id
                    }
                    onStartEdit={() =>
                      startRename(selectedEnvironment.id, "title")
                    }
                    onFinishEdit={(name) =>
                      void finishRename(selectedEnvironment.id, name)
                    }
                  />
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-7 shrink-0 border border-[var(--app-line)] text-[var(--app-dim)]"
                        title="More environment actions"
                      >
                        <Ellipsis className="size-3.5" />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent
                      align="end"
                      className="w-52"
                      // Every item moves focus on purpose (the name field, a
                      // new row, the delete prompt); returning it to this
                      // trigger would undo that. The menu holds focus until
                      // it has closed, so the new row is added only then.
                      onCloseAutoFocus={(event) => {
                        event.preventDefault();
                        if (!addAfterMenu.current) return;
                        addAfterMenu.current = false;
                        addVariable();
                      }}
                    >
                      <DropdownMenuItem
                        onSelect={() => {
                          addAfterMenu.current = true;
                        }}
                      >
                        <Plus />
                        New variable
                        <DropdownMenuShortcut>⌘N</DropdownMenuShortcut>
                      </DropdownMenuItem>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem
                        onSelect={() => void duplicateSelectedEnvironment()}
                      >
                        <Copy />
                        Duplicate environment
                        <DropdownMenuShortcut>⌘⇧D</DropdownMenuShortcut>
                      </DropdownMenuItem>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem
                        variant="destructive"
                        onSelect={() =>
                          void deleteEnvironment(selectedEnvironment.id)
                        }
                      >
                        <Trash2 />
                        Delete environment
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
                {table}
              </div>
            ) : null}
          </div>
        ) : (
          <div className="text-xs text-[var(--app-dim)]">
            <div>
              {selectedCollectionName ?? "This collection"} has no environments
              yet.
            </div>
            <div className="mt-3 flex gap-2">
              <Button
                variant="ghost"
                className={outlineButton}
                onClick={() => void createEnvironment()}
              >
                <Plus className="size-3.5" />
                New environment
                <Kbd>⌘⇧N</Kbd>
              </Button>
              <Button
                variant="ghost"
                className={outlineButton}
                onClick={() => environmentInputRef.current?.click()}
              >
                <Upload className="size-3.5" />
                Import
                <Kbd>⌘I</Kbd>
              </Button>
            </div>
          </div>
        )
      ) : null}
      {scope === "collection" && collections.length ? (
        <div
          className={cn(
            "grid gap-4",
            collections.length > 1 && "grid-cols-[184px_minmax(0,1fr)]",
          )}
        >
          {collections.length > 1 ? (
            <ListDrawer
              title="Collections"
              items={collections.map((collection) => ({
                id: collection.id,
                name: collection.name,
                markTitle:
                  collection.id === activeCollectionId
                    ? "Open in the sidebar"
                    : undefined,
              }))}
              selectedId={selectedCollectionId}
              onSelect={setPickedCollectionId}
            />
          ) : null}
          <div className="min-w-0">
            <div className="mb-3 -ml-2 flex min-h-8 items-center gap-2.5">
              {/* Collections are renamed on the Collections page. */}
              <EditableTitle
                name={selectedCollectionName ?? ""}
                label="Collection name"
              />
              {newVariableButton}
            </div>
            {table}
          </div>
        </div>
      ) : null}
    </div>
  );
}

// The row whose key or value field has focus.
function focusedVariableIndex() {
  const index = document.activeElement?.getAttribute("data-variable-index");
  return index ? Number(index) : undefined;
}

function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="mx-1 text-[10px] opacity-60">{children}</kbd>;
}

function SaveState({
  status,
  duplicate,
  onRetry,
}: {
  status: SaveStatus;
  duplicate?: string;
  onRetry: () => void;
}) {
  if (duplicate) {
    return (
      <span className="text-xs text-destructive">
        Not saved: “{duplicate}” is used twice
      </span>
    );
  }
  if (status === "failed") {
    return (
      <button className="text-xs text-destructive underline" onClick={onRetry}>
        Not saved. Retry
      </button>
    );
  }
  return (
    <span className="text-xs text-[var(--app-dim)]" aria-live="polite">
      {status === "saving" ? "Saving…" : status === "saved" ? "Saved" : ""}
    </span>
  );
}

function RowAction({
  className,
  title,
  onClick,
  children,
}: {
  className?: string;
  title: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <Button
      variant="ghost"
      size="icon"
      className={cn(
        "size-7 text-[var(--app-dim)] focus-visible:opacity-100 [&_svg]:size-3.5",
        className,
      )}
      onClick={onClick}
      title={title}
      aria-label={title}
    >
      {children}
    </Button>
  );
}
