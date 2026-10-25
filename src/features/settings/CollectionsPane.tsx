import { type ReactNode, useEffect, useRef, useState } from "react";
import {
  ChevronDown,
  Download,
  Library,
  Plus,
  Trash2,
  Upload,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import type {
  CollectionProfile,
  CollectionStats,
  CollectionSummary,
  MethodCount,
} from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { api } from "@/lib/tauri";
import {
  drawerAction,
  EditableTitle,
  ListDrawer,
  panelFrame as frame,
  panelLabel as label,
  SectionTitle,
} from "./SettingsParts";

export function CollectionsPane() {
  const collections = useWorkspaceStore((state) => state.collections);
  const activeCollectionId = useWorkspaceStore(
    (state) => state.activeCollectionId,
  );
  const deleteCollection = useWorkspaceStore((state) => state.deleteCollection);
  const renameCollection = useWorkspaceStore((state) => state.renameCollection);
  const createCollection = useWorkspaceStore((state) => state.createCollection);
  const pickAndImportCollection = useWorkspaceStore(
    (state) => state.pickAndImportCollection,
  );
  const setError = useWorkspaceStore((state) => state.setError);
  const [stats, setStats] = useState<Map<string, CollectionStats>>(
    () => new Map(),
  );
  // The page opens on the collection shown in the sidebar; a deleted
  // selection falls back to it as well.
  const [selectedId, setSelectedId] = useState(activeCollectionId);
  const selected =
    collections.find((collection) => collection.id === selectedId) ??
    collections.find((collection) => collection.id === activeCollectionId) ??
    collections[0];
  const [renaming, setRenaming] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  // The last import or export, shown here because the error banner is hidden
  // behind the Settings dialog.
  const [fileStatus, setFileStatus] = useState<{
    failed: boolean;
    text: string;
  } | null>(null);

  // Counts are read only while this page is open, and again whenever the
  // collection list changes.
  useEffect(() => {
    let current = true;
    api
      .listCollectionStats()
      .then((rows) => {
        if (current) setStats(new Map(rows.map((row) => [row.id, row])));
      })
      .catch(setError);
    return () => {
      current = false;
    };
  }, [collections, setError]);

  function select(collectionId: string) {
    setSelectedId(collectionId);
    setRenaming(false);
    setConfirmingDelete(false);
  }

  function startRename() {
    setRenaming(true);
    setConfirmingDelete(false);
  }

  function finishRename(collection: CollectionSummary, name: string | null) {
    setRenaming(false);
    const trimmed = name?.trim();
    if (!trimmed || trimmed === collection.name) return;
    void renameCollection(collection.id, trimmed);
  }

  // Like a new environment: made with a placeholder name and renamed at once.
  async function createAndRename() {
    const created = await createCollection("New collection");
    if (!created) return;
    select(created);
    startRename();
  }

  async function importAndSelect() {
    setFileStatus(null);
    try {
      const imported = await pickAndImportCollection();
      if (!imported) return;
      select(imported.id);
      setFileStatus({ failed: false, text: `Imported “${imported.name}”` });
    } catch (error) {
      setFileStatus({ failed: true, text: `Import failed: ${String(error)}` });
    }
  }

  async function exportCollection(
    collection: CollectionSummary,
    includeEnvironments: boolean,
  ) {
    setFileStatus(null);
    try {
      const fileName = await api.exportPostmanCollection(
        collection.id,
        includeEnvironments,
      );
      if (!fileName) return;
      setFileStatus({
        failed: false,
        text: includeEnvironments
          ? `Exported ${fileName} and its environments`
          : `Exported ${fileName}`,
      });
    } catch (error) {
      setFileStatus({ failed: true, text: `Export failed: ${String(error)}` });
    }
  }

  function askDelete() {
    setConfirmingDelete(true);
    setRenaming(false);
  }

  // While this pane is open, ⌘N makes a collection instead of a request, ⌘⇧N
  // and ⌘D do nothing rather than act on the hidden workspace (see
  // hotkeys.ts), and ⌘I, ⌘E and ⌘⌫ are handled here. Read through a ref so
  // the listeners see the current selection.
  const shortcuts = {
    new: () => void createAndRename(),
    import: () => void importAndSelect(),
    export: () => {
      if (selected) void exportCollection(selected, false);
    },
    remove: () => {
      if (selected) askDelete();
    },
  };
  const shortcutsRef = useRef(shortcuts);
  useEffect(() => {
    shortcutsRef.current = shortcuts;
  });
  useEffect(() => {
    function onNew(event: Event) {
      event.preventDefault();
      shortcutsRef.current.new();
    }
    function claim(event: Event) {
      event.preventDefault();
    }
    function onKeyDown(event: KeyboardEvent) {
      if (!(event.metaKey || event.ctrlKey) || event.shiftKey) return;
      // In a text field (the rename) these keep their editing meaning, such
      // as Ctrl+E for line end and ⌘⌫ for delete to line start.
      const target = event.target;
      if (
        target instanceof HTMLElement &&
        (target.isContentEditable || target.matches("input, textarea"))
      ) {
        return;
      }
      const key = event.key.toLowerCase();
      if (key === "i") {
        event.preventDefault();
        shortcutsRef.current.import();
      }
      if (key === "e") {
        event.preventDefault();
        shortcutsRef.current.export();
      }
      if (key === "backspace" || key === "delete") {
        event.preventDefault();
        shortcutsRef.current.remove();
      }
    }
    window.addEventListener("conductor:new", onNew);
    window.addEventListener("conductor:new-folder", claim);
    window.addEventListener("conductor:duplicate", claim);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("conductor:new", onNew);
      window.removeEventListener("conductor:new-folder", claim);
      window.removeEventListener("conductor:duplicate", claim);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  return (
    <div>
      <SectionTitle
        title="Collections"
        sub={
          collections.length
            ? `${plural(collections.length, "collection")}. Deleting one removes everything in it.`
            : undefined
        }
      />
      {fileStatus ? (
        <div
          className={cn(
            "-mt-1 mb-3 truncate text-xs",
            fileStatus.failed ? "text-destructive" : "text-[var(--app-dim)]",
          )}
          role="status"
          title={fileStatus.text}
        >
          {fileStatus.text}
        </div>
      ) : null}
      {selected ? (
        <div
          className={cn(
            "grid gap-4",
            collections.length > 1 && "grid-cols-[184px_minmax(0,1fr)]",
          )}
        >
          {collections.length > 1 ? (
            <ListDrawer
              title="All"
              items={collections.map((collection) => ({
                id: collection.id,
                name: collection.name,
                markTitle:
                  collection.id === activeCollectionId
                    ? "Open in the sidebar"
                    : undefined,
                aside: stats.get(collection.id)?.requestCount ?? "",
              }))}
              selectedId={selected.id}
              onSelect={select}
              createTitle="New collection (⌘N)"
              onCreate={() => void createAndRename()}
              actions={
                <Button
                  variant="ghost"
                  size="icon"
                  className={drawerAction}
                  title="Import Postman collection (⌘I)"
                  aria-label="Import collection"
                  onClick={() => void importAndSelect()}
                >
                  <Upload className="size-3.5" />
                </Button>
              }
            />
          ) : null}
          <CollectionDetail
            key={selected.id}
            collection={selected}
            stats={stats.get(selected.id)}
            renaming={renaming}
            confirmingDelete={confirmingDelete}
            onStartRename={startRename}
            onFinishRename={(name) => finishRename(selected, name)}
            onAskDelete={askDelete}
            onCancelDelete={() => setConfirmingDelete(false)}
            onDelete={() => {
              setConfirmingDelete(false);
              void deleteCollection(selected.id);
            }}
            onExport={(includeEnvironments) =>
              void exportCollection(selected, includeEnvironments)
            }
            // With one collection there is no list, so its buttons move here.
            onCreate={
              collections.length > 1 ? undefined : () => void createAndRename()
            }
            onImport={
              collections.length > 1 ? undefined : () => void importAndSelect()
            }
          />
        </div>
      ) : (
        <div
          className="flex flex-col items-center border border-dashed border-[var(--app-line)] px-6 py-10 text-center"
          style={{ borderRadius: "var(--app-radius-lg)" }}
        >
          <Library className="size-5 text-[var(--app-dim)]" />
          <div className="mt-3 text-sm font-medium">No collections yet</div>
          <div className="mt-1 text-xs text-[var(--app-dim)]">
            Create one, or import a Postman collection.
          </div>
          <div className="mt-4 flex gap-2">
            <Button
              variant="ghost"
              className="h-8 gap-1.5 border border-[var(--app-line)] px-2.5 text-xs text-[var(--app-dim)]"
              title="New collection (⌘N)"
              onClick={() => void createAndRename()}
            >
              <Plus className="size-3.5" />
              New collection
            </Button>
            <Button
              variant="ghost"
              className="h-8 gap-1.5 border border-[var(--app-line)] px-2.5 text-xs text-[var(--app-dim)]"
              title="Import Postman collection (⌘I)"
              onClick={() => void importAndSelect()}
            >
              <Upload className="size-3.5" />
              Import
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

function CollectionDetail({
  collection,
  stats,
  renaming,
  confirmingDelete,
  onStartRename,
  onFinishRename,
  onAskDelete,
  onCancelDelete,
  onDelete,
  onExport,
  onCreate,
  onImport,
}: {
  collection: CollectionSummary;
  stats: CollectionStats | undefined;
  renaming: boolean;
  confirmingDelete: boolean;
  onStartRename: () => void;
  // `null` cancels the rename.
  onFinishRename: (name: string | null) => void;
  onAskDelete: () => void;
  onCancelDelete: () => void;
  onDelete: () => void;
  onExport: (includeEnvironments: boolean) => void;
  onCreate?: () => void;
  onImport?: () => void;
}) {
  // Only the selected collection's profile is ever read; this component
  // remounts per selection, so the state belongs to one collection.
  const [profile, setProfile] = useState<CollectionProfile | null>(null);
  const setError = useWorkspaceStore((state) => state.setError);
  useEffect(() => {
    let current = true;
    api
      .getCollectionProfile(collection.id)
      .then((result) => {
        if (current) setProfile(result);
      })
      .catch(setError);
    return () => {
      current = false;
    };
  }, [collection.id, setError]);

  const requestCount =
    profile?.methods.reduce((sum, entry) => sum + entry.count, 0) ?? 0;
  const environmentCount = profile?.environments.length;

  const variableCount = profile
    ? profile.collectionVariableCount +
      profile.environments.reduce((sum, e) => sum + e.variableCount, 0)
    : undefined;

  return (
    <div className="min-w-0">
      <div className="mb-3 -ml-2 flex min-h-8 items-center gap-2.5">
        <EditableTitle
          name={collection.name}
          label="Collection name"
          editing={renaming}
          onStartEdit={onStartRename}
          onFinishEdit={onFinishRename}
        />
        {renaming || !onImport ? null : (
          <Button
            variant="ghost"
            size="icon"
            className="size-7 shrink-0 border border-[var(--app-line)] text-[var(--app-dim)] hover:text-[var(--app-text)]"
            title="Import Postman collection (⌘I)"
            aria-label="Import collection"
            onClick={onImport}
          >
            <Upload className="size-3.5" />
          </Button>
        )}
        {renaming || !onCreate ? null : (
          <Button
            variant="ghost"
            size="icon"
            className="size-7 shrink-0 border border-[var(--app-line)] text-[var(--app-dim)] hover:text-[var(--app-text)]"
            title="New collection (⌘N)"
            aria-label="New collection"
            onClick={onCreate}
          >
            <Plus className="size-3.5" />
          </Button>
        )}
        {renaming ? null : (
          <div className="flex shrink-0">
            <Button
              variant="ghost"
              className="h-7 gap-1.5 rounded-r-none border border-[var(--app-line)] px-2 text-xs text-[var(--app-dim)] hover:text-[var(--app-text)]"
              title="Export as a Postman collection (⌘E)"
              onClick={() => onExport(false)}
            >
              <Download className="size-3.5" />
              Export
            </Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-6 rounded-l-none border border-l-0 border-[var(--app-line)] text-[var(--app-dim)] hover:text-[var(--app-text)]"
                  title="Export options"
                  aria-label="Export options"
                >
                  <ChevronDown className="size-3.5" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-64">
                <DropdownMenuItem onSelect={() => onExport(false)}>
                  <Download />
                  <ExportOption
                    title="Collection"
                    detail=".postman_collection.json"
                  />
                  <DropdownMenuShortcut>⌘E</DropdownMenuShortcut>
                </DropdownMenuItem>
                <DropdownMenuItem
                  disabled={!environmentCount}
                  onSelect={() => onExport(true)}
                >
                  <Download />
                  <ExportOption
                    title="Collection + environments"
                    detail={
                      environmentCount === undefined
                        ? "Loading…"
                        : environmentCount
                          ? `also ${plural(environmentCount, "environment file")}, secrets included`
                          : "No environments to export"
                    }
                  />
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        )}
        {renaming ? null : (
          <Button
            variant="ghost"
            className="h-7 shrink-0 gap-1.5 border border-[var(--app-line)] px-2 text-xs text-[var(--app-dim)] hover:bg-destructive/10 hover:text-destructive"
            title="Delete collection (⌘⌫)"
            onClick={onAskDelete}
            aria-expanded={confirmingDelete}
          >
            <Trash2 className="size-3.5" />
            Delete
          </Button>
        )}
      </div>

      {confirmingDelete ? (
        <div
          className="mb-3 border border-destructive/40 bg-destructive/10 px-3 py-2.5 text-xs"
          style={{ borderRadius: "var(--app-radius-lg)" }}
          role="alertdialog"
          aria-label={`Delete ${collection.name}`}
        >
          <div className="text-sm">
            Delete{" "}
            <span className="font-semibold text-destructive">
              {collection.name}
            </span>
            ?
          </div>
          <div className="mt-0.5 text-[var(--app-dim)]">
            {stats
              ? `${plural(stats.requestCount, "request")}, ${plural(stats.folderCount, "folder")}, ${plural(stats.environmentCount, "environment")} and their variables go with it. `
              : "Its requests, folders, environments and variables go with it. "}
            This cannot be undone.
          </div>
          <div className="mt-2.5 flex justify-end gap-2">
            <Button
              variant="ghost"
              className="h-7 border border-[var(--app-line)] px-2.5 text-xs text-[var(--app-dim)]"
              onClick={onCancelDelete}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              className="h-7 px-2.5 text-xs"
              autoFocus
              onClick={onDelete}
            >
              Delete collection
            </Button>
          </div>
        </div>
      ) : null}

      <div className="mb-3 grid grid-cols-[repeat(auto-fit,minmax(128px,1fr))] gap-2">
        <Tile value={stats?.requestCount} label="Requests" />
        <Tile value={stats?.folderCount} label="Folders" />
        <Tile value={stats?.environmentCount} label="Environments" />
        <Tile value={variableCount} label="Variables" />
      </div>

      {profile ? (
        <>
          {profile.methods.length ? (
            <Section title="Methods" aside={plural(requestCount, "request")}>
              <MethodMix methods={profile.methods} total={requestCount} />
            </Section>
          ) : null}
          {profile.environments.length || profile.collectionVariableCount ? (
            <Section title="Environments" aside="variables">
              {profile.environments.map((environment) => (
                <Row key={environment.id}>
                  <span className="min-w-0 flex-1 truncate">
                    {environment.name}
                  </span>
                  <Mono>{environment.variableCount}</Mono>
                </Row>
              ))}
              <Row className="text-[var(--app-dim)]">
                <span className="flex-1">Collection-level</span>
                <Mono>{profile.collectionVariableCount}</Mono>
              </Row>
            </Section>
          ) : null}
          <Section title="Details">
            <dl className="text-xs">
              <Fact label="Created" value={formatDate(profile.createdAt)} />
              <Fact
                label="Last edited"
                value={
                  profile.lastEditedAt ? formatDate(profile.lastEditedAt) : null
                }
              />
              <Fact
                label="Deepest nesting"
                value={
                  profile.deepestNesting
                    ? plural(profile.deepestNesting, "folder")
                    : "flat"
                }
              />
              <Fact
                label="Largest folder"
                value={
                  profile.largestFolder
                    ? `${profile.largestFolder.name} · ${profile.largestFolder.requestCount}`
                    : null
                }
              />
              <Fact label="Latest request" value={profile.latestRequest} />
              <Fact
                label="URLs using variables"
                value={String(profile.variableUrlCount)}
              />
            </dl>
          </Section>
        </>
      ) : (
        <div
          className={cn(
            "px-3 py-6 text-center text-xs text-[var(--app-dim)]",
            frame,
          )}
          style={{ borderRadius: "var(--app-radius-lg)" }}
          role="status"
        >
          Crunching some numbers for ya…
        </div>
      )}
    </div>
  );
}

function ExportOption({ title, detail }: { title: string; detail: string }) {
  return (
    <span className="min-w-0">
      <span className="block text-xs">{title}</span>
      <span className="block truncate text-[11px] text-[var(--app-dim)]">
        {detail}
      </span>
    </span>
  );
}

function Tile({
  value,
  label: text,
}: {
  value: number | undefined;
  label: string;
}) {
  return (
    <div
      className={cn("px-2.5 py-2", frame)}
      style={{ borderRadius: "var(--app-radius-lg)" }}
    >
      {/* Holds the height while the counts load. */}
      <div className="app-mono text-xl leading-none font-semibold">
        {value ?? "–"}
      </div>
      <div className={cn("mt-1.5 truncate", label)}>{text}</div>
    </div>
  );
}

function Section({
  title,
  aside,
  children,
}: {
  title: string;
  aside?: string;
  children: ReactNode;
}) {
  return (
    <div
      className={cn("mb-2 px-3 pt-2.5 pb-2", frame)}
      style={{ borderRadius: "var(--app-radius-lg)" }}
    >
      <div className={cn("mb-2 flex justify-between", label)}>
        <span>{title}</span>
        {aside ? <span className="font-normal">{aside}</span> : null}
      </div>
      {children}
    </div>
  );
}

function Row({
  className,
  children,
}: {
  className?: string;
  children: ReactNode;
}) {
  return (
    <div className={cn("flex h-6 items-center gap-2 text-xs", className)}>
      {children}
    </div>
  );
}

function Mono({ children }: { children: ReactNode }) {
  return (
    <span className="app-mono text-[11px] text-[var(--app-dim)]">
      {children}
    </span>
  );
}

function Fact({ label: text, value }: { label: string; value: string | null }) {
  return (
    <div className="flex h-6 items-center justify-between gap-3 border-b border-[var(--app-line)] last:border-b-0">
      <dt className="shrink-0 text-[var(--app-dim)]">{text}</dt>
      <dd className="app-mono min-w-0 truncate text-[var(--app-text)]">
        {value ?? "–"}
      </dd>
    </div>
  );
}

// A stacked bar in the sidebar's method colours; `data-method` picks the
// colour through the rules in index.css.
function MethodMix({
  methods,
  total,
}: {
  methods: MethodCount[];
  total: number;
}) {
  return (
    <>
      <div className="mb-2 flex h-2 gap-px overflow-hidden rounded-[2px]">
        {methods.map((entry) => (
          <div
            key={entry.method}
            data-method={entry.method}
            className="h-full bg-[var(--method-fg,var(--app-dim))]"
            style={{ width: `${(entry.count / total) * 100}%` }}
            title={`${entry.method} ${entry.count}`}
          />
        ))}
      </div>
      <div className="app-mono flex flex-wrap gap-x-3.5 gap-y-1 text-[11px] text-[var(--app-dim)]">
        {methods.map((entry) => (
          <span key={entry.method} className="inline-flex items-center gap-1.5">
            <span
              data-method={entry.method}
              className="size-1.5 rounded-[1px] bg-[var(--method-fg,var(--app-dim))]"
            />
            {entry.method}{" "}
            <span className="font-semibold text-[var(--app-text)]">
              {entry.count}
            </span>
          </span>
        ))}
      </div>
    </>
  );
}

// Timestamps are RFC 3339; rows from old imports can carry an empty one,
// which shows as nothing rather than a date.
function formatDate(iso: string): string | null {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  return date.toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

function plural(count: number, noun: string) {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}
