import { type ReactNode, useEffect, useRef, useState } from "react";
import { Library, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type {
  CollectionProfile,
  CollectionStats,
  CollectionSummary,
} from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { api } from "@/lib/tauri";
import {
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
  // Escape unmounts the input, and the blur that follows must not save it.
  const renamingRef = useRef(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

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
    renamingRef.current = false;
    setRenaming(false);
    setConfirmingDelete(false);
  }

  function startRename() {
    renamingRef.current = true;
    setRenaming(true);
    setConfirmingDelete(false);
  }

  function finishRename(collection: CollectionSummary, name: string | null) {
    if (!renamingRef.current) return;
    renamingRef.current = false;
    setRenaming(false);
    const trimmed = name?.trim();
    if (!trimmed || trimmed === collection.name) return;
    void renameCollection(collection.id, trimmed);
  }

  async function confirmDelete(collection: CollectionSummary) {
    setConfirmingDelete(false);
    await deleteCollection(collection.id);
  }

  // Like a new environment: made with a placeholder name and renamed at once.
  async function createAndRename() {
    await createCollection("New collection");
    const created = useWorkspaceStore.getState().activeCollectionId;
    if (!created) return;
    select(created);
    startRename();
  }

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
              createTitle="New collection"
              onCreate={() => void createAndRename()}
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
            onAskDelete={() => {
              setConfirmingDelete(true);
              setRenaming(false);
              renamingRef.current = false;
            }}
            onCancelDelete={() => setConfirmingDelete(false)}
            onDelete={() => void confirmDelete(selected)}
            setError={setError}
            // With one collection there is no list, so its + moves here.
            onCreate={
              collections.length > 1 ? undefined : () => void createAndRename()
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
            Create one here, or import a Postman file from the sidebar.
          </div>
          <Button
            variant="ghost"
            className="mt-4 h-8 gap-1.5 border border-[var(--app-line)] px-2.5 text-xs text-[var(--app-dim)]"
            onClick={() => void createAndRename()}
          >
            <Plus className="size-3.5" />
            New collection
          </Button>
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
  setError,
  onCreate,
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
  setError: (error: unknown) => void;
  onCreate?: () => void;
}) {
  // Only the selected collection's profile is ever read; this component
  // remounts per selection, so the state belongs to one collection.
  const [profile, setProfile] = useState<CollectionProfile | null>(null);
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
        {renaming || !onCreate ? null : (
          <Button
            variant="ghost"
            size="icon"
            className="size-7 shrink-0 border border-[var(--app-line)] text-[var(--app-dim)] hover:text-[var(--app-text)]"
            title="New collection"
            aria-label="New collection"
            onClick={onCreate}
          >
            <Plus className="size-3.5" />
          </Button>
        )}
        {renaming ? null : (
          <Button
            variant="ghost"
            className="h-7 shrink-0 gap-1.5 border border-[var(--app-line)] px-2 text-xs text-[var(--app-dim)] hover:bg-destructive/10 hover:text-destructive"
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
            <Section
              title="Methods"
              aside={plural(stats?.requestCount ?? 0, "request")}
            >
              <MethodMix profile={profile} />
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
function MethodMix({ profile }: { profile: CollectionProfile }) {
  const total = profile.methods.reduce((sum, m) => sum + m.count, 0);
  return (
    <>
      <div className="mb-2 flex h-2 gap-px overflow-hidden rounded-[2px]">
        {profile.methods.map((entry) => (
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
        {profile.methods.map((entry) => (
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
