import type { CollectionNode } from "@/features/types";

export function insertNodeAt(
  nodes: CollectionNode[],
  parentId: string | null,
  position: number,
  node: CollectionNode,
): CollectionNode[] {
  if (parentId === null) {
    return insertIntoSiblings(nodes, position, { ...node, parentId: null });
  }

  let changed = false;
  const next = nodes.map((item) => {
    if (item.id === parentId) {
      changed = true;
      return {
        ...item,
        children: insertIntoSiblings(item.children, position, {
          ...node,
          parentId,
        }),
      };
    }
    if (!item.children.length) return item;
    const children = insertNodeAt(item.children, parentId, position, node);
    return children === item.children ? item : { ...item, children };
  });
  return changed || next.some((item, index) => item !== nodes[index]) ? next : nodes;
}

export function insertIntoSiblings(
  siblings: CollectionNode[],
  position: number,
  node: CollectionNode,
): CollectionNode[] {
  const index = Math.max(0, Math.min(position, siblings.length));
  const next = [...siblings.slice(0, index), node, ...siblings.slice(index)];
  return reindexSiblings(next);
}

export function removeNodeById(
  nodes: CollectionNode[],
  nodeId: string,
): CollectionNode[] {
  let changed = false;
  const next: CollectionNode[] = [];
  for (const node of nodes) {
    if (node.id === nodeId) {
      changed = true;
      continue;
    }
    if (node.children.length) {
      const children = removeNodeById(node.children, nodeId);
      if (children !== node.children) {
        changed = true;
        next.push({ ...node, children });
        continue;
      }
    }
    next.push(node);
  }
  return changed ? reindexSiblings(next) : nodes;
}

export function removeRequestNode(
  nodes: CollectionNode[],
  requestId: string,
): CollectionNode[] {
  let changed = false;
  const next: CollectionNode[] = [];
  for (const node of nodes) {
    if (node.requestId === requestId) {
      changed = true;
      continue;
    }
    if (node.children.length) {
      const children = removeRequestNode(node.children, requestId);
      if (children !== node.children) {
        changed = true;
        next.push({ ...node, children });
        continue;
      }
    }
    next.push(node);
  }
  return changed ? reindexSiblings(next) : nodes;
}

export function moveNodeInTree(
  nodes: CollectionNode[],
  node: CollectionNode,
  parentId: string | null,
  position: number,
): CollectionNode[] {
  const withoutNode = removeNodeById(nodes, node.id);
  return insertNodeAt(withoutNode, parentId, position, {
    ...node,
    parentId,
    position,
  });
}

export function findRequestNodeByRequestId(
  nodes: CollectionNode[],
  requestId: string,
): CollectionNode | undefined {
  for (const node of nodes) {
    if (node.requestId === requestId) return node;
    const child = findRequestNodeByRequestId(node.children, requestId);
    if (child) return child;
  }
  return undefined;
}

export function findNodeById(
  nodes: CollectionNode[],
  id: string,
): CollectionNode | undefined {
  for (const node of nodes) {
    if (node.id === id) return node;
    const child = findNodeById(node.children, id);
    if (child) return child;
  }
  return undefined;
}

export function reindexSiblings(nodes: CollectionNode[]): CollectionNode[] {
  return nodes.map((node, position) =>
    node.position === position ? node : { ...node, position },
  );
}

export function requestIdsForNode(node: CollectionNode): string[] {
  return [
    ...(node.requestId ? [node.requestId] : []),
    ...node.children.flatMap(requestIdsForNode),
  ];
}

export function renameRequestNode(
  nodes: CollectionNode[],
  requestId: string,
  name: string,
): CollectionNode[] {
  return nodes.map((node) => {
    if (node.requestId === requestId) {
      return { ...node, name };
    }
    if (node.children.length) {
      return {
        ...node,
        children: renameRequestNode(node.children, requestId, name),
      };
    }
    return node;
  });
}

export function collectNodeIds(node: CollectionNode): Set<string> {
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

export function containsNode(node: CollectionNode, id: string): boolean {
  if (node.id === id) return true;
  return node.children.some((child) => containsNode(child, id));
}

export function collectAllRequestIds(nodes: CollectionNode[]): string[] {
  return nodes.flatMap(requestIdsForNode);
}

export type FlattenedNode = {
  node: CollectionNode;
  depth: number;
};

export function flattenVisibleNodes(
  nodes: CollectionNode[],
  openIds: Set<string>,
  depth = 0,
  rows: FlattenedNode[] = [],
): FlattenedNode[] {
  for (const node of nodes) {
    rows.push({ node, depth });
    if (node.kind === "folder" && openIds.has(node.id)) {
      flattenVisibleNodes(node.children, openIds, depth + 1, rows);
    }
  }
  return rows;
}

export function filterTree(
  nodes: CollectionNode[],
  query: string,
): { tree: CollectionNode[]; matchedIds: Set<string> } {
  const normalized = query.trim().toLowerCase();
  const matchedIds = new Set<string>();
  if (!normalized) return { tree: nodes, matchedIds };

  const walk = (node: CollectionNode): CollectionNode | null => {
    const selfMatch = node.name.toLowerCase().includes(normalized);
    const nextChildren: CollectionNode[] = [];
    for (const child of node.children) {
      const kept = walk(child);
      if (kept) nextChildren.push(kept);
    }
    if (selfMatch || nextChildren.length) {
      matchedIds.add(node.id);
      return { ...node, children: nextChildren };
    }
    return null;
  };

  const tree: CollectionNode[] = [];
  for (const node of nodes) {
    const kept = walk(node);
    if (kept) tree.push(kept);
  }
  return { tree, matchedIds };
}
