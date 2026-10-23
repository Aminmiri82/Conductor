import { confirm } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { FileJson, Folder, Layers, Library, Trash2 } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { CollectionStats, CollectionSummary } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { api } from "@/lib/tauri";
import { SectionTitle } from "./SettingsParts";

export function CollectionsPane() {
  const collections = useWorkspaceStore((state) => state.collections);
  const activeCollectionId = useWorkspaceStore(
    (state) => state.activeCollectionId,
  );
  const deleteCollection = useWorkspaceStore((state) => state.deleteCollection);
  const setError = useWorkspaceStore((state) => state.setError);
  const [stats, setStats] = useState<Map<string, CollectionStats>>(
    () => new Map(),
  );

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

  // The dialog plugin replaces `window.confirm` with an async version whose
  // promise is always truthy, so the answer must be awaited from its API.
  async function confirmDelete(collection: CollectionSummary) {
    const confirmed = await confirm(
      `Delete collection "${collection.name}"? Its requests, folders, environments, and variables are deleted too. This cannot be undone.`,
      { kind: "warning" },
    );
    if (confirmed) await deleteCollection(collection.id);
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
      {collections.length ? (
        <div className="space-y-2">
          {collections.map((collection) => (
            <CollectionCard
              key={collection.id}
              collection={collection}
              stats={stats.get(collection.id)}
              active={collection.id === activeCollectionId}
              onDelete={() => void confirmDelete(collection)}
            />
          ))}
        </div>
      ) : (
        <div
          className="flex flex-col items-center border border-dashed border-[var(--app-line)] px-6 py-10 text-center"
          style={{ borderRadius: "var(--app-radius-lg)" }}
        >
          <Library className="size-5 text-[var(--app-dim)]" />
          <div className="mt-3 text-sm font-medium">No collections yet</div>
          <div className="mt-1 text-xs text-[var(--app-dim)]">
            Create or import one from the sidebar.
          </div>
        </div>
      )}
    </div>
  );
}

function CollectionCard({
  collection,
  stats,
  active,
  onDelete,
}: {
  collection: CollectionSummary;
  stats: CollectionStats | undefined;
  active: boolean;
  onDelete: () => void;
}) {
  return (
    <div
      className="group flex items-center gap-3 border border-[var(--app-line)] bg-[var(--app-panel)] px-3 py-3 transition-colors hover:border-[color-mix(in_oklab,var(--app-accent)_35%,var(--app-line))]"
      style={{ borderRadius: "var(--app-radius-lg)" }}
    >
      <div
        className="flex size-9 shrink-0 items-center justify-center bg-[color-mix(in_oklab,var(--app-accent)_14%,transparent)] text-[var(--app-accent)]"
        style={{ borderRadius: "var(--app-radius)" }}
      >
        <Library className="size-4" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <div className="truncate text-sm font-medium text-[var(--app-text)]">
            {collection.name}
          </div>
          {active ? (
            <span
              className="app-mono shrink-0 border border-[color-mix(in_oklab,var(--app-accent)_35%,transparent)] px-1.5 py-px text-[10px] uppercase tracking-[0.08em] text-[var(--app-accent)]"
              style={{ borderRadius: "var(--app-radius)" }}
            >
              Open
            </span>
          ) : null}
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-[var(--app-dim)]">
          <Count icon={FileJson} value={stats?.requestCount} noun="request" />
          <Count icon={Folder} value={stats?.folderCount} noun="folder" />
          <Count
            icon={Layers}
            value={stats?.environmentCount}
            noun="environment"
          />
        </div>
      </div>
      <Button
        variant="ghost"
        size="icon"
        className="size-8 shrink-0 text-[var(--app-dim)] opacity-60 group-hover:opacity-100 hover:bg-destructive/10 hover:text-destructive"
        title={`Delete ${collection.name}`}
        onClick={onDelete}
      >
        <Trash2 className="size-3.5" />
      </Button>
    </div>
  );
}

function Count({
  icon: Icon,
  value,
  noun,
}: {
  icon: LucideIcon;
  value: number | undefined;
  noun: string;
}) {
  return (
    <span className="inline-flex items-center gap-1">
      <Icon className="size-3 opacity-70" />
      {/* Holds the line's height while the counts load. */}
      {value === undefined ? `– ${noun}s` : plural(value, noun)}
    </span>
  );
}

function plural(count: number, noun: string) {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}
