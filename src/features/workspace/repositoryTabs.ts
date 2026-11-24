import type {
  CollectionNode,
  RepositoryRefresh,
  RequestNode,
} from "@/features/types";

export type RequestTab = {
  requestId: string;
  name: string;
  method: string;
  dirty: boolean;
  conflict?: { revision: string | null };
};

export function reconcileRepositoryTabs(
  tabs: RequestTab[],
  changes: RepositoryRefresh["changedRequests"],
  revisionFor: (requestId: string) => string | null | undefined,
  tree?: CollectionNode[],
) {
  const invalidate: string[] = [];
  const closed: string[] = [];
  if (!changes.length || !tabs.length) return { tabs, invalidate, closed };
  const revisions = new Map(
    changes.map((change) => [change.requestId, change.revision]),
  );
  let nodes: Map<string, RequestNode> | undefined;
  let tabsChanged = false;
  function collectRequests(
    branch: CollectionNode[],
    found: Map<string, RequestNode>,
  ) {
    for (const node of branch) {
      if (node.kind === "folder") collectRequests(node.children, found);
      else if (revisions.has(node.requestId)) found.set(node.requestId, node);
    }
  }
  const nextTabs = tabs.flatMap((tab) => {
    const revision = revisions.get(tab.requestId);
    if (revision === undefined) return [tab];
    if (revision !== null && revision === revisionFor(tab.requestId)) {
      if (!tab.conflict) return [tab];
      const { conflict: _conflict, ...restored } = tab;
      tabsChanged = true;
      return [restored];
    }
    if (tab.dirty) {
      if (tab.conflict?.revision === revision) return [tab];
      tabsChanged = true;
      return [{ ...tab, conflict: { revision } }];
    }
    invalidate.push(tab.requestId);
    if (revision === null) {
      tabsChanged = true;
      closed.push(tab.requestId);
      return [];
    }
    if (tree && !nodes) {
      nodes = new Map();
      collectRequests(tree, nodes);
    }
    const node = nodes?.get(tab.requestId);
    if (!node || (node.name === tab.name && node.method === tab.method)) {
      return [tab];
    }
    tabsChanged = true;
    return [{ ...tab, name: node.name, method: node.method }];
  });
  return { tabs: tabsChanged ? nextTabs : tabs, invalidate, closed };
}
