import {
  ChangeEvent,
  CSSProperties,
  DragEvent,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  ChevronDown,
  ChevronRight,
  FileJson,
  Folder,
  FolderOpen,
  Import,
  Plus,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
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
import { cn } from "@/lib/utils";
import type { CollectionNode } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

type DropIntent = "before" | "after" | "into";
type VisibleTreeNode = {
  node: CollectionNode;
  depth: number;
};
type DropTarget =
  | { kind: "node"; nodeId: string; intent: DropIntent }
  | { kind: "root-end" }
  | null;

type DnDValue = {
  draggingId: string | null;
  draggingNode: CollectionNode | null;
  draggingDescendantIds: Set<string>;
  beginDrag: (node: CollectionNode) => void;
  endDrag: () => void;
  target: DropTarget;
  setTarget: (target: DropTarget) => void;
};

const DnDContext = createContext<DnDValue | null>(null);
const TREE_ROW_HEIGHT = 28;
const TREE_OVERSCAN = 8;

function useDnD(): DnDValue {
  const value = useContext(DnDContext);
  if (!value) throw new Error("DnD context missing");
  return value;
}

export function CollectionSidebar() {
  const inputRef = useRef<HTMLInputElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
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

  const [draggingNode, setDraggingNode] = useState<CollectionNode | null>(null);
  const [draggingDescendantIds, setDraggingDescendantIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [target, setTarget] = useState<DropTarget>(null);
  const [openIds, setOpenIds] = useState<Set<string>>(() => new Set());
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(0);

  const dnd = useMemo<DnDValue>(
    () => ({
      draggingId: draggingNode?.id ?? null,
      draggingNode,
      draggingDescendantIds,
      beginDrag: (node) => {
        setDraggingNode(node);
        setDraggingDescendantIds(collectNodeIds(node));
        setTarget(null);
      },
      endDrag: () => {
        setDraggingNode(null);
        setDraggingDescendantIds(new Set());
        setTarget(null);
      },
      target,
      setTarget,
    }),
    [draggingDescendantIds, draggingNode, target],
  );

  const visibleNodes = useMemo(
    () => flattenVisibleNodes(tree, openIds),
    [openIds, tree],
  );
  const rootDropHeight = dnd.draggingId ? 32 : 8;
  const totalTreeHeight = visibleNodes.length * TREE_ROW_HEIGHT + rootDropHeight;
  const startIndex = Math.max(
    0,
    Math.floor(scrollTop / TREE_ROW_HEIGHT) - TREE_OVERSCAN,
  );
  const endIndex = Math.min(
    visibleNodes.length,
    Math.ceil((scrollTop + viewportHeight) / TREE_ROW_HEIGHT) + TREE_OVERSCAN,
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

  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) return;

    const updateSize = () => setViewportHeight(element.clientHeight);
    updateSize();
    const observer = new ResizeObserver(updateSize);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  async function onFileChange(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    await importCollection(text);
    event.target.value = "";
  }

  async function performDrop() {
    const source = draggingNode;
    const t = target;
    setDraggingNode(null);
    setTarget(null);
    if (!source || !t) return;

    let parentId: string | null | undefined;
    let position: number;

    if (t.kind === "root-end") {
      parentId = null;
      position = tree.length;
      if ((source.parentId ?? null) === null) position -= 1;
    } else {
      const node = findNode(tree, t.nodeId);
      if (!node) return;
      if (containsNode(source, node.id)) return;

      if (t.intent === "into") {
        parentId = node.id;
        position = node.children.length;
        if (source.parentId === node.id) position -= 1;
      } else {
        parentId = node.parentId ?? null;
        const sameParent = (source.parentId ?? null) === (parentId ?? null);
        const insertAt = t.intent === "before" ? node.position : node.position + 1;
        position =
          sameParent && source.position < insertAt ? insertAt - 1 : insertAt;
      }
    }

    if (position < 0) position = 0;
    if (
      (source.parentId ?? null) === (parentId ?? null) &&
      source.position === position
    ) {
      return;
    }

    await moveNode(source, parentId, position);
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

  return (
    <DnDContext.Provider value={dnd}>
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
                <DropdownMenuItem onSelect={() => void createRequestIn(null, tree.length)}>
                  Add request
                </DropdownMenuItem>
                <DropdownMenuItem onSelect={() => void createFolderIn(null, tree.length)}>
                  Add folder
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>

        <div
          ref={scrollRef}
          className="app-scroll min-h-0 flex-1 overflow-auto p-2"
          onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
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
                  activeRequestId={activeRequestId}
                  onToggleOpen={toggleOpen}
                  onOpenFolder={openFolder}
                  onSelectRequest={selectRequest}
                  onCreateRequest={createRequestIn}
                  onCreateFolder={createFolderIn}
                  onDuplicateRequest={duplicateRequest}
                  onDeleteRequest={confirmDeleteRequest}
                  onDeleteNode={confirmDeleteNode}
                  performDrop={performDrop}
                />
              ))}
              <RootEndDropZone
                top={visibleNodes.length * TREE_ROW_HEIGHT}
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
    </DnDContext.Provider>
  );
}

function RootEndDropZone({
  top,
  performDrop,
}: {
  top: number;
  performDrop: () => Promise<void>;
}) {
  const dnd = useDnD();
  const dragging = dnd.draggingId;
  const active = dnd.target?.kind === "root-end";

  if (!dragging) {
    return <div aria-hidden className="absolute h-2" style={{ top, left: 0, right: 0 }} />;
  }

  return (
    <div
      onDragOver={(event) => {
        event.preventDefault();
        event.stopPropagation();
        event.dataTransfer.dropEffect = "move";
        if (!active) dnd.setTarget({ kind: "root-end" });
      }}
      onDrop={(event) => {
        event.preventDefault();
        event.stopPropagation();
        void performDrop();
      }}
      className={cn(
        "absolute mx-1 mt-1 flex h-7 items-center justify-center border border-dashed text-[10px] uppercase tracking-[0.08em] transition-colors",
        active
          ? "border-[var(--app-accent)] bg-[color-mix(in_oklab,var(--app-accent)_10%,transparent)] text-[var(--app-accent)]"
          : "border-[var(--app-line)] text-[var(--app-dim)]/60",
      )}
      style={{ top, left: 0, right: 0, borderRadius: "var(--app-radius)" }}
    >
      Move to root
    </div>
  );
}

type TreeRowProps = {
  node: CollectionNode;
  depth: number;
  top: number;
  open: boolean;
  activeRequestId?: string;
  onToggleOpen: (nodeId: string) => void;
  onOpenFolder: (nodeId: string) => void;
  onSelectRequest: (requestId: string) => Promise<void>;
  onCreateRequest: (parentId: string | null | undefined, position: number) => Promise<void>;
  onCreateFolder: (parentId: string | null | undefined, position: number) => Promise<void>;
  onDuplicateRequest: (requestId: string) => Promise<void>;
  onDeleteRequest: (requestId: string) => Promise<void>;
  onDeleteNode: (node: CollectionNode) => Promise<void>;
  performDrop: () => Promise<void>;
};

function TreeRow({
  node,
  depth,
  top,
  open,
  activeRequestId,
  onToggleOpen,
  onOpenFolder,
  onSelectRequest,
  onCreateRequest,
  onCreateFolder,
  onDuplicateRequest,
  onDeleteRequest,
  onDeleteNode,
  performDrop,
}: TreeRowProps) {
  const dnd = useDnD();
  const isFolder = node.kind === "folder";
  const isActive = node.requestId === activeRequestId;
  const isDragging = dnd.draggingId === node.id;
  const isInvalidTarget = dnd.draggingDescendantIds.has(node.id);

  const target = dnd.target;
  const targetingMe = target?.kind === "node" && target.nodeId === node.id;
  const showBefore = targetingMe && target.intent === "before";
  const showAfter = targetingMe && target.intent === "after";
  const showInto = targetingMe && target.intent === "into";

  useEffect(() => {
    if (!showInto || !isFolder || open) return;
    const id = window.setTimeout(() => onOpenFolder(node.id), 600);
    return () => window.clearTimeout(id);
  }, [isFolder, node.id, onOpenFolder, open, showInto]);

  const indent = 8 + depth * 13;
  const lineInset = indent + 2;

  function onDragStart(event: DragEvent<HTMLDivElement>) {
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", node.id);
    dnd.beginDrag(node);
  }

  function onDragEnd() {
    dnd.endDrag();
  }

  function onDragOver(event: DragEvent<HTMLDivElement>) {
    if (!dnd.draggingId || isInvalidTarget) return;
    event.preventDefault();
    event.stopPropagation();
    event.dataTransfer.dropEffect = "move";

    const rect = event.currentTarget.getBoundingClientRect();
    const y = event.clientY - rect.top;
    const h = rect.height;

    let intent: DropIntent;
    if (isFolder) {
      if (y < h * 0.3) intent = "before";
      else if (y > h * 0.7) intent = "after";
      else intent = "into";
    } else {
      intent = y < h * 0.5 ? "before" : "after";
    }

    if (
      target?.kind !== "node" ||
      target.nodeId !== node.id ||
      target.intent !== intent
    ) {
      dnd.setTarget({ kind: "node", nodeId: node.id, intent });
    }
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    event.stopPropagation();
    void performDrop();
  }

  return (
    <div
      className="absolute left-0 right-0"
      style={{ top, height: TREE_ROW_HEIGHT }}
    >
      {showBefore ? <DropLine inset={lineInset} position="top" /> : null}
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            role="button"
            tabIndex={0}
            data-sidebar-request-id={node.requestId ?? undefined}
            draggable
            onDragStart={onDragStart}
            onDragEnd={onDragEnd}
            onDragOver={onDragOver}
            onDrop={onDrop}
            className={cn(
              "group relative flex h-7 w-full select-none items-center gap-1.5 pr-1 text-left text-xs text-[var(--app-dim)] transition-colors",
              "hover:bg-[var(--app-panel-2)] hover:text-[var(--app-text)]",
              isActive &&
                "bg-[color-mix(in_oklab,var(--app-accent)_14%,transparent)] text-[var(--app-text)] ring-1 ring-[color-mix(in_oklab,var(--app-accent)_28%,transparent)]",
              isDragging && "opacity-40",
              showInto &&
                "bg-[color-mix(in_oklab,var(--app-accent)_10%,transparent)] text-[var(--app-text)] ring-1 ring-[var(--app-accent)]",
            )}
            style={{ paddingLeft: indent, borderRadius: "var(--app-radius)" }}
            onClick={() => {
              if (isFolder) {
                onToggleOpen(node.id);
              } else if (node.requestId) {
                void onSelectRequest(node.requestId);
              }
            }}
          >
            {isFolder ? (
              open ? (
                <ChevronDown className="size-3.5 shrink-0" />
              ) : (
                <ChevronRight className="size-3.5 shrink-0" />
              )
            ) : (
              <span className="w-3.5 shrink-0" />
            )}
            {isFolder ? (
              open ? (
                <FolderOpen className="size-3.5 shrink-0 text-[var(--app-accent)] opacity-80" />
              ) : (
                <Folder className="size-3.5 shrink-0 text-[var(--app-accent)] opacity-70" />
              )
            ) : (
              <FileJson className="size-3.5 shrink-0 text-[var(--app-dim)]" />
            )}
            {!isFolder ? (
              <span
                className="app-mono w-10 shrink-0 px-1 py-0.5 text-center text-[10px] font-bold leading-none"
                style={methodColor(node.method ?? "GET")}
              >
                {node.method ?? "GET"}
              </span>
            ) : null}
            <span className="min-w-0 flex-1 truncate">{node.name}</span>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-44">
          {isFolder ? (
            <>
              <ContextMenuItem
                onSelect={() => void onCreateRequest(node.id, node.children.length)}
              >
                Add request
              </ContextMenuItem>
              <ContextMenuItem
                onSelect={() => void onCreateFolder(node.id, node.children.length)}
              >
                Add folder
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem
                variant="destructive"
                onSelect={() => void onDeleteNode(node)}
              >
                Delete folder
              </ContextMenuItem>
            </>
          ) : node.requestId ? (
            <>
              <ContextMenuItem
                onSelect={() => void onDuplicateRequest(node.requestId!)}
              >
                Duplicate request
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem
                variant="destructive"
                onSelect={() => void onDeleteRequest(node.requestId!)}
              >
                Delete request
              </ContextMenuItem>
            </>
          ) : null}
        </ContextMenuContent>
      </ContextMenu>
      {showAfter ? <DropLine inset={lineInset} position="bottom" /> : null}
    </div>
  );
}

function DropLine({
  inset,
  position,
}: {
  inset: number;
  position: "top" | "bottom";
}) {
  return (
    <div
      aria-hidden
      className={cn(
        "pointer-events-none absolute right-2 z-20 h-0.5 rounded-full bg-[var(--app-accent)]",
        position === "top" ? "-top-px" : "-bottom-px",
      )}
      style={{ left: inset }}
    >
      <span className="absolute -left-1 top-1/2 block size-2 -translate-y-1/2 rounded-full bg-[var(--app-accent)] ring-2 ring-[color-mix(in_oklab,var(--app-accent)_25%,transparent)]" />
    </div>
  );
}

function methodColor(method: string): CSSProperties {
  const normalized = method.toUpperCase();
  const map: Record<string, CSSProperties> = {
    GET: { color: "var(--app-get)", backgroundColor: "var(--app-get-bg)" },
    POST: { color: "var(--app-post)", backgroundColor: "var(--app-post-bg)" },
    PUT: { color: "var(--app-put)", backgroundColor: "var(--app-put-bg)" },
    DELETE: {
      color: "var(--app-delete)",
      backgroundColor: "var(--app-delete-bg)",
    },
    PATCH: { color: "var(--app-patch)", backgroundColor: "var(--app-patch-bg)" },
  };
  return {
    ...(map[normalized] ?? {
      color: "var(--app-text)",
      backgroundColor: "transparent",
    }),
    borderRadius: "var(--app-radius)",
  };
}

function flattenVisibleNodes(
  nodes: CollectionNode[],
  openIds: Set<string>,
  depth = 0,
  rows: VisibleTreeNode[] = [],
): VisibleTreeNode[] {
  for (const node of nodes) {
    rows.push({ node, depth });
    if (node.kind === "folder" && openIds.has(node.id)) {
      flattenVisibleNodes(node.children, openIds, depth + 1, rows);
    }
  }
  return rows;
}

function collectNodeIds(node: CollectionNode): Set<string> {
  const ids = new Set<string>();
  collectNodeIdsInto(node, ids);
  return ids;
}

function collectNodeIdsInto(node: CollectionNode, ids: Set<string>) {
  ids.add(node.id);
  for (const child of node.children) {
    collectNodeIdsInto(child, ids);
  }
}

function findNode(nodes: CollectionNode[], id: string): CollectionNode | undefined {
  for (const node of nodes) {
    if (node.id === id) return node;
    const child = findNode(node.children, id);
    if (child) return child;
  }
  return undefined;
}

function containsNode(node: CollectionNode, id: string): boolean {
  if (node.id === id) return true;
  return node.children.some((child) => containsNode(child, id));
}
