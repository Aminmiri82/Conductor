import type { RepositoryRefresh } from "@/features/types";

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
) {
  const invalidate: string[] = [];
  const closed: string[] = [];
  if (!changes.length || !tabs.length) return { tabs, invalidate, closed };
  const byId = new Map(changes.map((change) => [change.requestId, change]));
  let tabsChanged = false;
  const nextTabs = tabs.flatMap((tab) => {
    const change = byId.get(tab.requestId);
    if (!change) return [tab];
    const { revision, name, method } = change;
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
    if (
      name === null ||
      method === null ||
      (name === tab.name && method === tab.method)
    ) {
      return [tab];
    }
    tabsChanged = true;
    return [{ ...tab, name, method }];
  });
  return { tabs: tabsChanged ? nextTabs : tabs, invalidate, closed };
}
