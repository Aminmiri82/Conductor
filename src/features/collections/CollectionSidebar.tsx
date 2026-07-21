import { type ChangeEvent, useEffect, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import {
  Download,
  Import,
  MoreVertical,
  Pencil,
  Plus,
  Search,
  Trash2,
  X,
} from "lucide-react";
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
import type { CollectionNode } from "@/features/types";
import { api } from "@/lib/tauri";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { CollectionTree } from "@/features/collections/CollectionTree";
import { DnDProvider, useTreeDnDValue } from "@/features/collections/treeDnD";

export function CollectionSidebar() {
  const importInputRef = useRef<HTMLInputElement>(null);
  const collections = useWorkspaceStore((state) => state.collections);
  const tree = useWorkspaceStore((state) => state.tree);
  const activeCollectionId = useWorkspaceStore((state) => state.activeCollectionId);
  const activeRequestId = useWorkspaceStore((state) => state.activeRequestId);
  const selectCollection = useWorkspaceStore((state) => state.selectCollection);
  const selectRequest = useWorkspaceStore((state) => state.selectRequest);
  const importCollection = useWorkspaceStore((state) => state.importCollection);
  const createRequestIn = useWorkspaceStore((state) => state.createRequestIn);
  const createFolderIn = useWorkspaceStore((state) => state.createFolderIn);
  const duplicateRequest = useWorkspaceStore((state) => state.duplicateRequest);
  const deleteRequest = useWorkspaceStore((state) => state.deleteRequest);
  const deleteNode = useWorkspaceStore((state) => state.deleteNode);
  const moveNode = useWorkspaceStore((state) => state.moveNode);
  const renameCollection = useWorkspaceStore((state) => state.renameCollection);
  const deleteCollection = useWorkspaceStore((state) => state.deleteCollection);
  const exportCollection = useWorkspaceStore((state) => state.exportCollection);

  const [filter, setFilter] = useState("");
  const [renaming, setRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState("");
  const renameInputRef = useRef<HTMLInputElement>(null);
  const dnd = useTreeDnDValue();

  const activeCollection = collections.find(
    (collection) => collection.id === activeCollectionId,
  );

  useEffect(() => {
    if (!renaming) return;
    renameInputRef.current?.focus();
    renameInputRef.current?.select();
  }, [renaming]);

  async function onFileChange(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    await importCollection(text);
    event.target.value = "";
  }

  async function confirmDeleteRequest(requestId: string) {
    if (!window.confirm("Delete this request?")) return;
    await deleteRequest(requestId);
  }

  async function confirmDeleteNode(node: CollectionNode) {
    const message =
      node.kind === "folder"
        ? `Delete folder "${node.name}" and everything inside it?`
        : `Delete request "${node.name}"?`;
    if (!window.confirm(message)) return;
    await deleteNode(node);
  }

  function beginRename() {
    if (!activeCollection) return;
    setRenameValue(activeCollection.name);
    setRenaming(true);
  }

  async function commitRename() {
    if (!activeCollectionId) return;
    const trimmed = renameValue.trim();
    setRenaming(false);
    if (!trimmed || trimmed === activeCollection?.name) return;
    await renameCollection(activeCollectionId, trimmed);
  }

  function cancelRename() {
    setRenaming(false);
  }

  async function confirmDeleteCollection() {
    if (!activeCollectionId || !activeCollection) return;
    const confirmed = window.confirm(
      `Delete collection "${activeCollection.name}" and everything inside it? This cannot be undone.`,
    );
    if (!confirmed) return;
    await deleteCollection(activeCollectionId);
  }

  async function exportActiveCollection() {
    if (!activeCollectionId || !activeCollection) return;
    const suggestedName = `${activeCollection.name || "collection"}.postman_collection.json`;
    const path = await save({
      defaultPath: suggestedName,
      filters: [{ name: "Postman Collection", extensions: ["json"] }],
    });
    if (!path) return;
    const contents = await exportCollection(activeCollectionId);
    await api.saveTextFile(path, contents);
  }

  return (
    <DnDProvider value={dnd}>
      <aside className="flex h-full min-h-0 flex-col border-r border-[var(--app-line)] bg-[var(--app-panel)]">
        <div className="space-y-2 border-b border-[var(--app-line)] p-3">
          <div className="flex items-center justify-between">
            <div className="app-mono text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--app-dim)]">
              Collections
            </div>
            <Button
              variant="ghost"
              size="icon"
              className="size-7 text-[var(--app-dim)] hover:text-[var(--app-text)]"
              onClick={() => importInputRef.current?.click()}
              title="Import Postman collection"
            >
              <Import className="size-4" />
            </Button>
            <input
              ref={importInputRef}
              className="hidden"
              type="file"
              accept=".json,application/json"
              onChange={onFileChange}
            />
          </div>
          <div className="flex items-center gap-2">
            {renaming ? (
              <Input
                ref={renameInputRef}
                className="h-8 min-w-0 flex-1 border-[var(--app-line)] bg-[var(--app-panel-2)] text-xs"
                value={renameValue}
                onChange={(event) => setRenameValue(event.target.value)}
                onBlur={() => void commitRename()}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void commitRename();
                  } else if (event.key === "Escape") {
                    event.preventDefault();
                    cancelRename();
                  }
                }}
              />
            ) : (
              <Select
                value={activeCollectionId}
                onValueChange={(value) => void selectCollection(value)}
              >
                <SelectTrigger className="h-8 min-w-0 flex-1 border-[var(--app-line)] bg-[var(--app-panel-2)] text-xs">
                  <SelectValue placeholder="No collection" />
                </SelectTrigger>
                <SelectContent>
                  {collections.map((collection) => (
                    <SelectItem key={collection.id} value={collection.id}>
                      {collection.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-8 shrink-0 text-[var(--app-dim)]"
                  disabled={!activeCollectionId}
                  title="Add to collection"
                >
                  <Plus className="size-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-40">
                <DropdownMenuItem
                  onSelect={() => void createRequestIn(null, tree.length)}
                >
                  Add request
                </DropdownMenuItem>
                <DropdownMenuItem
                  onSelect={() => void createFolderIn(null, tree.length)}
                >
                  Add folder
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-8 shrink-0 text-[var(--app-dim)]"
                  disabled={!activeCollectionId}
                  title="Collection actions"
                >
                  <MoreVertical className="size-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-48">
                <DropdownMenuItem onSelect={beginRename}>
                  <Pencil className="mr-2 size-3.5" />
                  Rename collection
                </DropdownMenuItem>
                <DropdownMenuItem
                  onSelect={() => void exportActiveCollection()}
                >
                  <Download className="mr-2 size-3.5" />
                  Export as Postman
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  variant="destructive"
                  onSelect={() => void confirmDeleteCollection()}
                >
                  <Trash2 className="mr-2 size-3.5" />
                  Delete collection
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
          <div className="relative">
            <Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-[var(--app-dim)]" />
            <Input
              className="h-8 border-[var(--app-line)] bg-[var(--app-panel-2)] pl-7 pr-7 text-xs"
              placeholder="Search requests"
              value={filter}
              onChange={(event) => setFilter(event.target.value)}
              disabled={!tree.length}
            />
            {filter ? (
              <button
                type="button"
                className="absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-[var(--app-dim)] hover:text-[var(--app-text)]"
                onClick={() => setFilter("")}
                title="Clear search"
              >
                <X className="size-3.5" />
              </button>
            ) : null}
          </div>
        </div>

        <CollectionTree
          tree={tree}
          filter={filter}
          activeRequestId={activeRequestId}
          onSelectRequest={selectRequest}
          onCreateRequest={createRequestIn}
          onCreateFolder={createFolderIn}
          onDuplicateRequest={duplicateRequest}
          onDeleteRequest={confirmDeleteRequest}
          onDeleteNode={confirmDeleteNode}
          onMoveNode={moveNode}
        />
      </aside>
    </DnDProvider>
  );
}
