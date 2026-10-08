import {
  type ChangeEvent,
  useCallback,
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
import { RootEndDropZone, TREE_ROW_HEIGHT, TreeRow } from "./TreeRow";
import {
  collectNodeIds,
  flattenVisibleNodes,
  resolveDrop,
  type DropTarget,
} from "./tree";

const TREE_OVERSCAN = 8;

export function CollectionSidebar() {
  const inputRef = useRef<HTMLInputElement>(null);
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
  const duplicateRequest = useWorkspaceStore((state) => state.duplicateRequest);
  const deleteNode = useWorkspaceStore((state) => state.deleteNode);
  const moveNode = useWorkspaceStore((state) => state.moveNode);

  const [drag, setDrag] = useState<{
    source: CollectionNode;
    descendantIds: ReadonlySet<string>;
  } | null>(null);
  const [target, setTargetState] = useState<DropTarget>(null);
  const [openIds, setOpenIds] = useState<Set<string>>(() => new Set());
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

  async function onFileChange(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    await importCollection(text);
    event.target.value = "";
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
      if (!window.confirm(message)) return;
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
            onClick={() => inputRef.current?.click()}
            title="Import Postman collection"
          >
            <Import className="size-4" />
          </Button>
          <input
            ref={inputRef}
            className="hidden"
            type="file"
            accept=".json,application/json"
            onChange={onFileChange}
          />
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
                onToggleOpen={toggleOpen}
                onOpenFolder={openFolder}
                onSelectRequest={selectRequest}
                onCreateRequest={createRequestIn}
                onCreateFolder={createFolderIn}
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
        ) : (
          <div className="px-2 py-8 text-center text-xs text-[var(--app-dim)]">
            Import a Postman collection to begin.
          </div>
        )}
      </div>
    </aside>
  );
}
