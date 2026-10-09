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
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
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
import { Segmented, SectionTitle } from "./SettingsParts";

const columns = "grid-cols-[minmax(140px,.8fr)_minmax(180px,1.4fr)_120px]";

// Row actions stay out of the way until the row is hovered or focused.
const onRowHover =
  "opacity-0 group-focus-within:opacity-100 group-hover:opacity-100";

const pickerClass =
  "h-8 w-44 border-[var(--app-line)] bg-[var(--app-panel-2)] text-xs";

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
  const [selectedCollectionId, setSelectedCollectionId] = useState(
    activeCollectionId ?? collections[0]?.id ?? "",
  );
  const activeEnvironmentId = useWorkspaceUiStore((state) =>
    activeEnvironmentIdFor(state.workspaceUi, selectedCollectionId),
  );
  const environments = allEnvironments.filter(
    (environment) => environment.collectionId === selectedCollectionId,
  );
  const [selectedEnvironmentId, setSelectedEnvironmentId] = useState(
    activeEnvironmentId ?? environments[0]?.id ?? "",
  );
  const selectedEnvironment = environments.find(
    (environment) => environment.id === selectedEnvironmentId,
  );
  const [variables, setVariables] = useState<VariableEntry[]>([]);
  const [status, setStatus] = useState<SaveStatus>("idle");
  const { save: saveVariables } = useSaveVariables();
  const session = useRef<Session | null>(null);
  // The row whose key field takes focus when it appears (a just-added row).
  const [focusRow, setFocusRow] = useState<number | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [draftName, setDraftName] = useState("");
  const [revealed, setRevealed] = useState<Record<number, boolean>>({});
  const environmentInputRef = useRef<HTMLInputElement>(null);
  const nameInputRef = useRef<HTMLInputElement>(null);

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
    if (selectedCollectionId || !collections[0]) return;
    setSelectedCollectionId(collections[0].id);
  }, [collections, selectedCollectionId]);

  // Show the collection's active environment, or its first one.
  const environmentIds = environments.map((environment) => environment.id);
  const environmentIdsKey = environmentIds.join("\n");
  useEffect(() => {
    if (activeEnvironmentId) {
      setSelectedEnvironmentId(activeEnvironmentId);
      return;
    }
    if (environmentIds.includes(selectedEnvironmentId)) return;
    setSelectedEnvironmentId(environmentIds[0] ?? "");
    // environmentIdsKey stands in for environmentIds, a new array each render.
    // oxlint-disable-next-line react/exhaustive-deps
  }, [activeEnvironmentId, environmentIdsKey, selectedEnvironmentId]);

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

  function removeVariable(index: number) {
    const rows = session.current?.rows ?? variables;
    edit(
      rows.filter((_, i) => i !== index),
      "now",
    );
    setRevealed((current) =>
      Object.fromEntries(
        Object.entries(current)
          .filter(([key]) => Number(key) !== index)
          .map(([key, value]) => [
            Number(key) > index ? String(Number(key) - 1) : key,
            value,
          ]),
      ),
    );
  }

  function addVariable() {
    if (!target) return;
    const rows = session.current?.rows ?? variables;
    setFocusRow(rows.length);
    edit(
      [...rows, { key: "", value: "", enabled: true, sensitive: false }],
      "typing",
    );
  }

  // ⌘N adds a row while this pane is open instead of a request (see
  // hotkeys.ts). Read through a ref so the listener sees the current table.
  const addVariableRef = useRef(addVariable);
  useEffect(() => {
    addVariableRef.current = addVariable;
  });
  useEffect(() => {
    function onNew(event: Event) {
      event.preventDefault();
      addVariableRef.current();
    }
    window.addEventListener("conductor:new", onNew);
    return () => window.removeEventListener("conductor:new", onNew);
  }, []);

  function chooseEnvironment(environmentId: string) {
    setSelectedEnvironmentId(environmentId);
    void selectEnvironment(selectedCollectionId, environmentId);
  }

  function startRename(name = selectedEnvironment?.name ?? "") {
    setDraftName(name);
    setRenaming(true);
  }

  async function commitRename() {
    // Escape unmounts the input first; a blur after that is not a commit.
    if (!renaming) return;
    setRenaming(false);
    const name = draftName.trim();
    if (!selectedEnvironmentId || !name || name === selectedEnvironment?.name) {
      return;
    }
    try {
      await api.renameEnvironment(selectedEnvironmentId, name);
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
      startRename("New environment");
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
      startRename(`${selectedEnvironment?.name ?? "Environment"} copy`);
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

  async function deleteSelectedEnvironment() {
    if (!selectedEnvironmentId) return;
    const confirmed = window.confirm(
      `Delete environment "${selectedEnvironment?.name ?? "selected environment"}"?`,
    );
    if (!confirmed) return;

    const nextEnvironmentId =
      environments.find(
        (environment) => environment.id !== selectedEnvironmentId,
      )?.id ?? null;
    try {
      await api.deleteEnvironment(selectedEnvironmentId);
      await loadEnvironments();
      setSelectedEnvironmentId(nextEnvironmentId ?? "");
      await selectEnvironment(selectedCollectionId, nextEnvironmentId);
    } catch (error) {
      useWorkspaceStore.getState().setError(error);
    }
  }

  const duplicates = duplicateKeys(variables);
  const selectedCollectionName = collections.find(
    (collection) => collection.id === selectedCollectionId,
  )?.name;

  return (
    <div>
      <div className="flex items-start gap-4">
        <SectionTitle
          title="Variables"
          sub="Each collection has its own environments. Environment variables override collection variables, which override globals."
        />
        <div className="ml-auto flex shrink-0 items-center gap-3">
          <SaveState
            status={status}
            duplicate={[...duplicates][0]}
            onRetry={() => void autosaver.flush()}
          />
          {scope === "environment" && selectedCollectionId ? (
            <Button
              variant="ghost"
              className="h-8 gap-1.5 border border-[var(--app-line)] px-2.5 text-xs text-[var(--app-dim)]"
              onClick={() => environmentInputRef.current?.click()}
              title="Import a Postman environment or .env file into this collection"
            >
              <Upload className="size-3.5" />
              Import
            </Button>
          ) : null}
          <input
            ref={environmentInputRef}
            type="file"
            accept="application/json,.json,.env,text/plain"
            className="hidden"
            onChange={(event) => void onEnvironmentFileChange(event)}
          />
        </div>
      </div>
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <Segmented
          value={scope}
          options={[
            ["environment", "Environment"],
            ["collection", "Collection"],
            ["global", "Global"],
          ]}
          onChange={setScope}
        />
        {scope !== "global" ? (
          <Select
            value={selectedCollectionId}
            onValueChange={setSelectedCollectionId}
            disabled={!collections.length}
          >
            <SelectTrigger className={pickerClass} aria-label="Collection">
              <SelectValue placeholder="No collections" />
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
      </div>
      {scope === "environment" && selectedCollectionId ? (
        <div className="mb-3 flex flex-wrap items-center gap-2">
          {renaming ? (
            <Input
              ref={nameInputRef}
              autoFocus
              className={pickerClass}
              value={draftName}
              onChange={(event) => setDraftName(event.target.value)}
              onFocus={(event) => event.target.select()}
              onBlur={() => void commitRename()}
              onKeyDown={(event) => {
                if (event.key === "Enter") event.currentTarget.blur();
                if (event.key === "Escape") setRenaming(false);
              }}
              data-cancel-on-escape
              aria-label="Environment name"
            />
          ) : (
            <Select
              value={selectedEnvironmentId}
              onValueChange={chooseEnvironment}
              disabled={!environments.length}
            >
              <SelectTrigger className={pickerClass} aria-label="Environment">
                <SelectValue placeholder="No environments" />
              </SelectTrigger>
              <SelectContent>
                {environments.map((environment) => (
                  <SelectItem key={environment.id} value={environment.id}>
                    {environment.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
          <Button
            variant="ghost"
            className="h-8 gap-1.5 border border-[var(--app-line)] px-2.5 text-xs text-[var(--app-dim)]"
            onClick={() => void createEnvironment()}
          >
            <Plus className="size-3.5" />
            New
          </Button>
          <Button
            variant="ghost"
            className="h-8 gap-1.5 border border-[var(--app-line)] px-2.5 text-xs text-[var(--app-dim)]"
            onClick={() => void duplicateSelectedEnvironment()}
            disabled={!selectedEnvironmentId}
          >
            <Copy className="size-3.5" />
            Duplicate
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="size-8 border border-[var(--app-line)] text-[var(--app-dim)]"
                title="More environment actions"
              >
                <Ellipsis className="size-3.5" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              align="start"
              className="w-52"
              // Rename swaps the picker for a name field; keep focus there
              // instead of returning it to this menu's trigger.
              onCloseAutoFocus={(event) => {
                if (!renaming) return;
                event.preventDefault();
                nameInputRef.current?.focus();
              }}
            >
              <DropdownMenuItem
                disabled={!selectedEnvironmentId}
                onSelect={() => startRename()}
              >
                <Pencil />
                Rename
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                variant="destructive"
                disabled={!selectedEnvironmentId}
                onSelect={() => void deleteSelectedEnvironment()}
              >
                <Trash2 />
                Delete environment
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      ) : (
        <div className="mb-3" />
      )}
      {target ? (
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
                    autoFocus={index === focusRow}
                    className={cn(
                      !variable.enabled && "text-[var(--app-dim)] line-through",
                      duplicate && "text-destructive",
                    )}
                    value={variable.key}
                    placeholder="Key"
                    title={duplicate ? "This key is used twice" : undefined}
                    onChange={(event) =>
                      updateVariable(
                        index,
                        { key: event.target.value },
                        "typing",
                      )
                    }
                  />
                  <CellInput
                    className={cn(
                      !variable.enabled && "text-[var(--app-dim)] line-through",
                    )}
                    type={
                      variable.sensitive && !revealed[index]
                        ? "password"
                        : "text"
                    }
                    value={variable.value}
                    placeholder="Value"
                    onChange={(event) =>
                      updateVariable(
                        index,
                        { value: event.target.value },
                        "typing",
                      )
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
                      title="Remove variable"
                    >
                      <Trash2 />
                    </RowAction>
                  </div>
                </Row>
              );
            })}
          </div>
          <button
            className="flex h-9 w-full items-center gap-2 px-3 text-left text-xs text-[var(--app-dim)] hover:bg-[var(--app-panel-2)] hover:text-[var(--app-text)]"
            onClick={addVariable}
          >
            <Plus className="size-3.5" />
            Add variable
            <kbd className="ml-1 text-[10px] opacity-60">⌘N</kbd>
          </button>
        </TableFrame>
      ) : (
        <div className="text-xs text-[var(--app-dim)]">
          {!collections.length
            ? "Import a collection to add collection and environment variables."
            : `${selectedCollectionName ?? "This collection"} has no environments yet. Create one with New, or Import one.`}
        </div>
      )}
    </div>
  );
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
