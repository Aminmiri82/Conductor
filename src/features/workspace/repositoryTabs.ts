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
  if (!changes.length) return { tabs, invalidate: [] };
  const revisions = new Map(
    changes.map((change) => [change.requestId, change.revision]),
  );
  const invalidate: string[] = [];
  let tabsChanged = false;
  const nextTabs = tabs.flatMap((tab) => {
    const revision = revisions.get(tab.requestId);
    if (revision === undefined || revision === revisionFor(tab.requestId)) {
      return [tab];
    }
    if (tab.dirty) {
      if (tab.conflict?.revision === revision) return [tab];
      tabsChanged = true;
      return [{ ...tab, conflict: { revision } }];
    }
    invalidate.push(tab.requestId);
    if (revision !== null) return [tab];
    tabsChanged = true;
    return [];
  });
  return { tabs: tabsChanged ? nextTabs : tabs, invalidate };
}
