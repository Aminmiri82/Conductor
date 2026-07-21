import { useCallback, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { CollectionNode } from "@/features/types";
import {
  filterTree,
  findNodeById,
  flattenVisibleNodes,
} from "@/features/workspace/collectionTree";
import { useDnD } from "@/features/collections/treeDnD";
import {
  RootEndDropZone,
  TreeRow,
  TREE_ROW_HEIGHT,
} from "@/features/collections/TreeNode";

const TREE_OVERSCAN = 8;

export type CollectionTreeProps = {
  tree: CollectionNode[];
  filter: string;
  activeRequestId?: string;
  onSelectRequest: (requestId: string) => Promise<void>;
  onCreateRequest: (
    parentId: string | null | undefined,
    position: number,
  ) => Promise<void>;
  onCreateFolder: (
    parentId: string | null | undefined,
    position: number,
  ) => Promise<void>;
  onDuplicateRequest: (requestId: string) => Promise<void>;
  onDeleteRequest: (requestId: string) => Promise<void>;
  onDeleteNode: (node: CollectionNode) => Promise<void>;
  onMoveNode: (
    node: CollectionNode,
    parentId: string | null | undefined,
    position: number,
  ) => Promise<void>;
};

export function CollectionTree(props: CollectionTreeProps) {
  const {
    tree,
    filter,
    activeRequestId,
    onSelectRequest,
    onCreateRequest,
    onCreateFolder,
    onDuplicateRequest,
    onDeleteRequest,
    onDeleteNode,
    onMoveNode,
  } = props;
  const dnd = useDnD();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [openIds, setOpenIds] = useState<Set<string>>(() => new Set());
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(0);

  const { tree: filteredTree, matchedIds } = useMemo(
    () => filterTree(tree, filter),
    [filter, tree],
  );

  const filtering = filter.trim().length > 0;

  const effectiveOpenIds = useMemo(() => {
    if (!filtering) return openIds;
    const merged = new Set(openIds);
    matchedIds.forEach((id) => merged.add(id));
    return merged;
  }, [filtering, matchedIds, openIds]);

  const visibleNodes = useMemo(
    () => flattenVisibleNodes(filteredTree, effectiveOpenIds),
    [effectiveOpenIds, filteredTree],
  );

  const rootDropHeight = dnd.draggingId ? 32 : 8;
  const totalTreeHeight =
    visibleNodes.length * TREE_ROW_HEIGHT + rootDropHeight;
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

  async function performDrop() {
    const source = dnd.draggingNode;
    const t = dnd.target;
    dnd.endDrag();
    if (!source || !t) return;

    let parentId: string | null | undefined;
    let position: number;

    if (t.kind === "root-end") {
      parentId = null;
      position = tree.length;
      if ((source.parentId ?? null) === null) position -= 1;
    } else {
      const node = findNodeById(tree, t.nodeId);
      if (!node) return;
      if (dnd.draggingDescendantIds.has(node.id)) return;

      if (t.intent === "into") {
        parentId = node.id;
        position = node.children.length;
        if (source.parentId === node.id) position -= 1;
      } else {
        parentId = node.parentId ?? null;
        const sameParent = (source.parentId ?? null) === (parentId ?? null);
        const insertAt =
          t.intent === "before" ? node.position : node.position + 1;
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

    await onMoveNode(source, parentId, position);
  }

  if (!tree.length) {
    return (
      <div className="app-scroll min-h-0 flex-1 overflow-auto p-2">
        <div className="px-2 py-8 text-center text-xs text-[var(--app-dim)]">
          Import a Postman collection to begin.
        </div>
      </div>
    );
  }

  if (!filteredTree.length) {
    return (
      <div className="app-scroll min-h-0 flex-1 overflow-auto p-2">
        <div className="px-2 py-8 text-center text-xs text-[var(--app-dim)]">
          No requests match your search.
        </div>
      </div>
    );
  }

  return (
    <div
      ref={scrollRef}
      className="app-scroll min-h-0 flex-1 overflow-auto p-2"
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
      onDragLeave={(event) => {
        if (event.currentTarget.contains(event.relatedTarget as Node)) return;
        dnd.setTarget(null);
      }}
    >
      <div className="relative" style={{ height: totalTreeHeight }}>
        {renderedNodes.map((item, index) => (
          <TreeRow
            key={item.node.id}
            node={item.node}
            depth={item.depth}
            top={(startIndex + index) * TREE_ROW_HEIGHT}
            open={effectiveOpenIds.has(item.node.id)}
            activeRequestId={activeRequestId}
            onToggleOpen={toggleOpen}
            onOpenFolder={openFolder}
            onSelectRequest={onSelectRequest}
            onCreateRequest={onCreateRequest}
            onCreateFolder={onCreateFolder}
            onDuplicateRequest={onDuplicateRequest}
            onDeleteRequest={onDeleteRequest}
            onDeleteNode={onDeleteNode}
            performDrop={performDrop}
          />
        ))}
        <RootEndDropZone
          top={visibleNodes.length * TREE_ROW_HEIGHT}
          performDrop={performDrop}
        />
      </div>
    </div>
  );
}
