import type {
  CollectionNode,
  RequestDetail,
  RequestNode,
} from "@/features/types";

// Pure helpers for the collection tree. Siblings carry a dense `position`
// (0..n-1); Rust stores sparse sort orders, so every edit here reindexes the
// affected sibling lists to match what the next `get_collection_tree` returns.

export type DropIntent = "before" | "after" | "into";
export type DropTarget =
  | { kind: "node"; nodeId: string; intent: DropIntent }
  | { kind: "root-end" }
  | null;
export type VisibleTreeNode = { node: CollectionNode; depth: number };
export type TreePlacement = { parentId: string | null; position: number };

function findNode(
  nodes: CollectionNode[],
  matches: (node: CollectionNode) => boolean,
): CollectionNode | undefined {
  for (const node of nodes) {
    if (matches(node)) return node;
    if (node.kind === "folder") {
      const child = findNode(node.children, matches);
      if (child) return child;
    }
  }
  return undefined;
}

export function findNodeById(nodes: CollectionNode[], id: string) {
  return findNode(nodes, (node) => node.id === id);
}

export function findRequestNode(
  nodes: CollectionNode[],
  requestId: string,
): RequestNode | undefined {
  const node = findNode(
    nodes,
    (node) => node.kind === "request" && node.requestId === requestId,
  );
  return node?.kind === "request" ? node : undefined;
}

export function collectNodeIds(node: CollectionNode): Set<string> {
  const ids = new Set<string>();
  const visit = (current: CollectionNode) => {
    ids.add(current.id);
    if (current.kind === "folder") current.children.forEach(visit);
  };
  visit(node);
  return ids;
}

export function requestIdsForNode(node: CollectionNode): string[] {
  return node.kind === "request"
    ? [node.requestId]
    : node.children.flatMap(requestIdsForNode);
}

export function flattenVisibleNodes(
  nodes: CollectionNode[],
  openIds: ReadonlySet<string>,
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

// Where a new request goes so it lands right after `requestId`, or at the end
// of the collection root when that request is not in the tree.
export function placementAfterRequest(
  tree: CollectionNode[],
  requestId: string | undefined,
): TreePlacement {
  const node = requestId ? findRequestNode(tree, requestId) : undefined;
  return node
    ? { parentId: node.parentId ?? null, position: node.position + 1 }
    : { parentId: null, position: tree.length };
}

// Turns a drop into the arguments of `moveNode`. Positions are indexes among
// the new siblings *after* the dragged node has been taken out, which is what
// Rust's `move_node` expects. Returns null when the drop changes nothing.
export function resolveDrop(
  tree: CollectionNode[],
  source: CollectionNode,
  target: DropTarget,
): TreePlacement | null {
  if (!target) return null;
  const sourceParentId = source.parentId ?? null;

  let parentId: string | null;
  let position: number;

  if (target.kind === "root-end") {
    parentId = null;
    position = tree.length;
    if (sourceParentId === null) position -= 1;
  } else {
    const node = findNodeById(tree, target.nodeId);
    if (!node) return null;

    if (target.intent === "into") {
      if (node.kind !== "folder") return null;
      parentId = node.id;
      position = node.children.length;
      if (sourceParentId === node.id) position -= 1;
    } else {
      parentId = node.parentId ?? null;
      const insertAt =
        target.intent === "before" ? node.position : node.position + 1;
      position =
        sourceParentId === parentId && source.position < insertAt
          ? insertAt - 1
          : insertAt;
    }
  }

  position = Math.max(0, position);
  if (sourceParentId === parentId && source.position === position) return null;
  return { parentId, position };
}

function reindexSiblings(nodes: CollectionNode[]): CollectionNode[] {
  return nodes.map((node, position) =>
    node.position === position ? node : { ...node, position },
  );
}

function insertIntoSiblings(
  siblings: CollectionNode[],
  position: number,
  node: CollectionNode,
): CollectionNode[] {
  const index = Math.max(0, Math.min(position, siblings.length));
  return reindexSiblings([
    ...siblings.slice(0, index),
    node,
    ...siblings.slice(index),
  ]);
}

export function insertNodeAt(
  nodes: CollectionNode[],
  parentId: string | null,
  position: number,
  node: CollectionNode,
): CollectionNode[] {
  if (parentId === null) {
    return insertIntoSiblings(nodes, position, { ...node, parentId: null });
  }

  const next = nodes.map((item) => {
    if (item.kind !== "folder") return item;
    if (item.id === parentId) {
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
  return next.some((item, index) => item !== nodes[index]) ? next : nodes;
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
    if (node.kind === "folder" && node.children.length) {
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

export function moveNodeInTree(
  nodes: CollectionNode[],
  node: CollectionNode,
  parentId: string | null,
  position: number,
): CollectionNode[] {
  return insertNodeAt(removeNodeById(nodes, node.id), parentId, position, {
    ...node,
    parentId,
    position,
  });
}

// Returns the same array when nothing changed, so saving without a rename
// does not re-render everything that reads the tree.
export function updateRequestNode(
  nodes: CollectionNode[],
  request: Pick<RequestDetail, "id" | "name" | "method">,
): CollectionNode[] {
  let changed = false;
  const next = nodes.map((node) => {
    if (node.kind === "request" && node.requestId === request.id) {
      if (node.name === request.name && node.method === request.method) {
        return node;
      }
      changed = true;
      return { ...node, name: request.name, method: request.method };
    }
    if (node.kind !== "folder" || !node.children.length) return node;
    const children = updateRequestNode(node.children, request);
    if (children === node.children) return node;
    changed = true;
    return { ...node, children };
  });
  return changed ? next : nodes;
}
