import { confirm, open } from "@tauri-apps/plugin-dialog";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Import, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { CollectionNode } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import {
  focusRenameInput,
  RootEndDropZone,
  TREE_ROW_HEIGHT,
  TreeRow,
} from "./TreeRow";
import {
  collectNodeIds,
  findNodeById,
  flattenVisibleNodes,
  resolveDrop,
  type DropTarget,
} from "./tree";

const TREE_OVERSCAN = 8;

// Not on the first screen; loads the first time it opens.
const NewCollectionDialog = lazy(() =>
  import("./NewCollectionDialog").then((module) => ({
    default: module.NewCollectionDialog,
  })),
);

export function CollectionSidebar() {
  const scrollRef = useRef<HTMLDivElement>(null);
  const collections = useWorkspaceStore((state) => state.collections);
  const tree = useWorkspaceStore((state) => state.tree);
  const activeCollectionId = useWorkspaceStore(
    (state) => state.activeCollectionId,
  );
  const activeRequestId = useWorkspaceStore((state) => state.activeRequestId);
  const selectCollection = useWorkspaceStore((state) => state.selectCollection);
  const selectRequest = useWorkspaceStore((state) => state.selectRequest);
  const importCollection = useWorkspaceStore((state) => state.importCollection);
  const createRequestIn = useWorkspaceStore((state) => state.createRequestIn);
  const createFolderIn = useWorkspaceStore((state) => state.createFolderIn);
  const renameFolder = useWorkspaceStore((state) => state.renameFolder);
  const pendingRenameId = useWorkspaceStore((state) => state.pendingRenameId);
  const clearPendingRename = useWorkspaceStore(
    (state) => state.clearPendingRename,
  );
  const duplicateRequest = useWorkspaceStore((state) => state.duplicateRequest);
  const deleteNode = useWorkspaceStore((state) => state.deleteNode);
  const moveNode = useWorkspaceStore((state) => state.moveNode);

  const [drag, setDrag] = useState<{
    source: CollectionNode;
    descendantIds: ReadonlySet<string>;
  } | null>(null);
  const [target, setTargetState] = useState<DropTarget>(null);
  const [openIds, setOpenIds] = useState<Set<string>>(() => new Set());
  const [renamingId, setRenamingId] = useState<string | null>(null);
  // Read by the stable finish callback: Escape clears the rename, and the
  // input's blur as it unmounts must not then save it.
  const renamingIdRef = useRef<string | null>(null);
  const renameOnMenuClose = useRef(false);
  const [newCollectionOpen, setNewCollectionOpen] = useState(false);
  const [newCollectionOpened, setNewCollectionOpened] = useState(false);
  // First and last row touching the viewport. The scroll position itself lives
  // in refs; state only changes when a row enters or leaves, not on every
  // scroll event.
  const [visibleRows, setVisibleRows] = useState({ first: 0, last: 0 });
  const scrollTopRef = useRef(0);
  // Mirrors `drag` and `target` so the drop handlers can stay referentially
  // stable and keep the memoized rows from re-rendering.
  const dragRef = useRef<{ source: CollectionNode | null; target: DropTarget }>(
    {
      source: null,
      target: null,
    },
  );

  const visibleNodes = useMemo(
    () => flattenVisibleNodes(tree, openIds),
    [openIds, tree],
  );
  const dragActive = drag !== null;
  const rootDropHeight = dragActive ? 32 : 8;
  const totalTreeHeight =
    visibleNodes.length * TREE_ROW_HEIGHT + rootDropHeight;
  const startIndex = Math.max(0, visibleRows.first - TREE_OVERSCAN);
  const endIndex = Math.min(
    visibleNodes.length,
    visibleRows.last + TREE_OVERSCAN,
  );
  const renderedNodes = visibleNodes.slice(startIndex, endIndex);

  const toggleOpen = useCallback((nodeId: string) => {
    setOpenIds((current) => {
      const next = new Set(current);
      if (next.has(nodeId)) {
        next.delete(nodeId);
      } else {
        next.add(nodeId);
      }
      return next;
    });
  }, []);

  const openFolder = useCallback((nodeId: string) => {
    setOpenIds((current) => {
      if (current.has(nodeId)) return current;
      const next = new Set(current);
      next.add(nodeId);
      return next;
    });
  }, []);

  const startRename = useCallback((nodeId: string) => {
    renamingIdRef.current = nodeId;
    setRenamingId(nodeId);
  }, []);

  const finishRename = useCallback(
    (nodeId: string, name: string | null) => {
      if (renamingIdRef.current !== nodeId) return;
      renamingIdRef.current = null;
      setRenamingId(null);
      const trimmed = name?.trim();
      const current = findNodeById(useWorkspaceStore.getState().tree, nodeId);
      if (!trimmed || trimmed === current?.name) return;
      void renameFolder(nodeId, trimmed);
    },
    [renameFolder],
  );

  // A new folder opens straight into rename, since it starts as "New Folder".
  const createFolderAndRename = useCallback(
    async (parentId: string | null | undefined, position: number) => {
      const nodeId = await createFolderIn(parentId, position);
      if (!nodeId) return;
      if (parentId) openFolder(parentId);
      startRename(nodeId);
    },
    [createFolderIn, openFolder, startRename],
  );

  useEffect(() => {
    if (!pendingRenameId) return;
    clearPendingRename();
    const parentId = findNodeById(
      useWorkspaceStore.getState().tree,
      pendingRenameId,
    )?.parentId;
    if (parentId) openFolder(parentId);
    startRename(pendingRenameId);
  }, [pendingRenameId, clearPendingRename, openFolder, startRename]);

  const syncVisibleRows = useCallback(() => {
    const element = scrollRef.current;
    if (!element) return;
    const top = scrollTopRef.current;
    const first = Math.floor(top / TREE_ROW_HEIGHT);
    const last = Math.ceil((top + element.clientHeight) / TREE_ROW_HEIGHT);
    setVisibleRows((current) =>
      current.first === first && current.last === last
        ? current
        : { first, last },
    );
  }, []);

  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) return;

    scrollTopRef.current = element.scrollTop;
    syncVisibleRows();
    const observer = new ResizeObserver(syncVisibleRows);
    observer.observe(element);
    return () => observer.disconnect();
  }, [syncVisibleRows]);

  const setTarget = useCallback((next: DropTarget) => {
    dragRef.current.target = next;
    setTargetState(next);
  }, []);

  const beginDrag = useCallback((node: CollectionNode) => {
    dragRef.current = { source: node, target: null };
    setDrag({ source: node, descendantIds: collectNodeIds(node) });
    setTargetState(null);
  }, []);

  const endDrag = useCallback(() => {
    dragRef.current = { source: null, target: null };
    setDrag(null);
    setTargetState(null);
  }, []);

  function openNewCollection() {
    setNewCollectionOpened(true);
    setNewCollectionOpen(true);
  }

  async function pickCollectionFile() {
    const path = await open({
      filters: [{ name: "Postman collection", extensions: ["json"] }],
    });
    if (path) await importCollection(path);
  }

  const performDrop = useCallback(async () => {
    const { source, target: dropTarget } = dragRef.current;
    endDrag();
    if (!source) return;
    const placement = resolveDrop(
      useWorkspaceStore.getState().tree,
      source,
      dropTarget,
    );
    if (placement) {
      await moveNode(source, placement.parentId, placement.position);
    }
  }, [endDrag, moveNode]);

  const confirmDeleteNode = useCallback(
    async (node: CollectionNode) => {
      const message =
        node.kind === "folder"
          ? `Delete folder "${node.name}" and everything inside it?`
          : `Delete request "${node.name}"?`;
      if (!(await confirm(message, { kind: "warning" }))) return;
      await deleteNode(node);
    },
    [deleteNode],
  );

  return (
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
            onClick={() => void pickCollectionFile()}
            title="Import Postman collection"
          >
            <Import className="size-4" />
          </Button>
        </div>
        <div className="flex items-center gap-2">
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
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="size-8 shrink-0 text-[var(--app-dim)]"
                title="Add"
              >
                <Plus className="size-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              align="end"
              className="w-40"
              onCloseAutoFocus={(event) => {
                if (!renameOnMenuClose.current) return;
                renameOnMenuClose.current = false;
                focusRenameInput(event);
              }}
            >
              <DropdownMenuItem
                disabled={!activeCollectionId}
                onSelect={() => void createRequestIn(null, tree.length)}
              >
                Add request
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!activeCollectionId}
                onSelect={() => {
                  renameOnMenuClose.current = true;
                  void createFolderAndRename(null, tree.length);
                }}
              >
                Add folder
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={openNewCollection}>
                New collection
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      <div
        ref={scrollRef}
        className="app-scroll min-h-0 flex-1 overflow-auto p-2"
        onScroll={(event) => {
          scrollTopRef.current = event.currentTarget.scrollTop;
          syncVisibleRows();
        }}
        onDragLeave={(event) => {
          if (event.currentTarget.contains(event.relatedTarget as Node)) return;
          setTarget(null);
        }}
      >
        {tree.length ? (
          <div className="relative" style={{ height: totalTreeHeight }}>
            {renderedNodes.map((item, index) => (
              <TreeRow
                key={item.node.id}
                node={item.node}
                depth={item.depth}
                top={(startIndex + index) * TREE_ROW_HEIGHT}
                open={openIds.has(item.node.id)}
                active={
                  item.node.kind === "request" &&
                  item.node.requestId === activeRequestId
                }
                dropIntent={
                  target?.kind === "node" && target.nodeId === item.node.id
                    ? target.intent
                    : null
                }
                dragActive={dragActive}
                isDragging={drag?.source.id === item.node.id}
                isInvalidTarget={drag?.descendantIds.has(item.node.id) ?? false}
                renaming={renamingId === item.node.id}
                onStartRename={startRename}
                onFinishRename={finishRename}
                onToggleOpen={toggleOpen}
                onOpenFolder={openFolder}
                onSelectRequest={selectRequest}
                onCreateRequest={createRequestIn}
                onCreateFolder={createFolderAndRename}
                onDuplicateRequest={duplicateRequest}
                onDeleteNode={confirmDeleteNode}
                onBeginDrag={beginDrag}
                onEndDrag={endDrag}
                onSetTarget={setTarget}
                performDrop={performDrop}
              />
            ))}
            <RootEndDropZone
              top={visibleNodes.length * TREE_ROW_HEIGHT}
              dragActive={dragActive}
              active={target?.kind === "root-end"}
              onSetTarget={setTarget}
              performDrop={performDrop}
            />
          </div>
        ) : activeCollectionId ? (
          <div className="px-2 py-8 text-center text-xs text-[var(--app-dim)]">
            This collection is empty.
            <div className="mt-3">
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-xs"
                onClick={() => void createRequestIn(null, 0)}
              >
                Add request
              </Button>
            </div>
          </div>
        ) : (
          <div className="px-2 py-8 text-center text-xs text-[var(--app-dim)]">
            Create a collection, or import one from Postman.
            <div className="mt-3 flex justify-center gap-2">
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-xs"
                onClick={openNewCollection}
              >
                New collection
              </Button>
              <Button
                variant="ghost"
                size="sm"
                className="h-7 text-xs"
                onClick={() => void pickCollectionFile()}
              >
                Import
              </Button>
            </div>
          </div>
        )}
      </div>
      <Suspense>
        {newCollectionOpened ? (
          <NewCollectionDialog
            open={newCollectionOpen}
            onOpenChange={setNewCollectionOpen}
          />
        ) : null}
      </Suspense>
    </aside>
  );
}
