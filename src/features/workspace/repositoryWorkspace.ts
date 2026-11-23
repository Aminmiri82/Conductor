import type { SaveRequestResult } from "@/bindings";
import type { RepositoryRefresh, RequestDetail } from "@/features/types";
import { api } from "@/lib/tauri";
import { getDraft, useDraftStore } from "./draftStore";
import { reconcileRepositoryTabs } from "./repositoryTabs";
import { useResponseStore } from "./responseStore";
import {
  closeRequestTab,
  normalizeRequestParams,
  saveDraft,
  upsertTab,
  useWorkspaceStore,
} from "./workspaceStore";

type ConflictAction = "reload" | "keep" | "save-as-new";

export async function applyRepositoryRefresh({
  collections,
  changedRequests,
  changedCollectionIds,
}: RepositoryRefresh) {
  const initial = useWorkspaceStore.getState();
  if (
    collections.length !== initial.collections.length ||
    collections.some((collection, index) => {
      const previous = initial.collections[index];
      return (
        collection.id !== previous.id ||
        collection.name !== previous.name ||
        collection.repository?.root !== previous.repository?.root ||
        collection.repository?.issue !== previous.repository?.issue
      );
    })
  ) {
    useWorkspaceStore.setState({ collections });
  }
  if (!changedCollectionIds.length && !changedRequests.length) return;
  const collectionId = initial.activeCollectionId;
  const activeCollectionChanged =
    collectionId && changedCollectionIds.includes(collectionId);
  const [tree] = await Promise.all([
    activeCollectionChanged ? api.getCollectionTree(collectionId) : undefined,
    changedCollectionIds.length ? initial.loadEnvironments() : undefined,
  ]);
  const current = useWorkspaceStore.getState();
  if (current.activeCollectionId !== collectionId) return;

  const { tabs, invalidate } = reconcileRepositoryTabs(
    current.tabs,
    changedRequests,
    (requestId) => getDraft(requestId)?.repositoryRevision,
  );
  useDraftStore.getState().removeMany(invalidate);
  useResponseStore
    .getState()
    .removeMany(
      invalidate.filter((id) => !tabs.some((tab) => tab.requestId === id)),
    );
  useWorkspaceStore.setState({
    tabs,
    ...(tree && current.tree === initial.tree ? { tree } : {}),
  });
  const active = current.activeRequestId;
  if (!active) return;
  const activeTab = tabs.find((tab) => tab.requestId === active);
  if (invalidate.includes(active)) {
    const next = activeTab ?? tabs[0];
    if (next) await current.selectRequest(next.requestId);
    else useWorkspaceStore.setState({ activeRequestId: undefined });
  } else if (
    activeCollectionChanged &&
    activeTab?.conflict?.revision !== null
  ) {
    await current.resolveActiveRequest();
  }
}

export async function applySaveConflict(
  requestId: string,
  result: Extract<SaveRequestResult, { status: "conflict" }>,
) {
  if (result.refresh) await applyRepositoryRefresh(result.refresh);
  useWorkspaceStore.setState((state) => ({
    tabs: state.tabs.map((tab) =>
      tab.requestId === requestId
        ? { ...tab, conflict: { revision: result.revision } }
        : tab,
    ),
  }));
}

export async function resolveRepositoryConflict(action: ConflictAction) {
  const { getState: get, setState: set } = useWorkspaceStore;
  const request = getDraft(get().activeRequestId);
  if (!request || get().saving) return;
  const conflict = get().tabs.find(
    (tab) => tab.requestId === request.id,
  )?.conflict;
  if (!conflict) return;
  if (action === "keep" && conflict.revision !== null) {
    await saveDraft(request, conflict.revision);
    return;
  }
  if (action === "reload" && conflict.revision === null) {
    await closeRequestTab(request.id);
    return;
  }
  set({ error: undefined, saving: true });
  try {
    if (action === "save-as-new") {
      await saveDraftAsNew(request);
    } else {
      const current = normalizeRequestParams(await api.getRequest(request.id));
      if (getDraft(request.id) !== request) return;
      useDraftStore.getState().setDraft(current);
      set((state) => ({ tabs: upsertTab(state.tabs, current, false) }));
    }
    await get().resolveActiveRequest();
  } catch (error) {
    set({ error: String(error) });
  } finally {
    set({ saving: false });
  }
}

async function saveDraftAsNew(request: RequestDetail) {
  const { requestId } = await api.saveRepositoryRequestAsNew(request);
  const [tree, saved] = await Promise.all([
    api.getCollectionTree(request.collectionId),
    api.getRequest(requestId),
  ]);
  const current = useWorkspaceStore.getState();
  const draft = getDraft(request.id);
  if (current.activeCollectionId !== request.collectionId || !draft) return;
  const next =
    draft === request
      ? normalizeRequestParams(saved)
      : {
          ...draft,
          id: requestId,
          repositoryRevision: saved.repositoryRevision,
        };
  useDraftStore.getState().removeMany([request.id]);
  useResponseStore.getState().removeMany([request.id]);
  useDraftStore.getState().setDraft(next);
  useWorkspaceStore.setState({
    tree,
    activeRequestId:
      current.activeRequestId === request.id
        ? requestId
        : current.activeRequestId,
    tabs: upsertTab(
      current.tabs.filter((tab) => tab.requestId !== request.id),
      next,
      draft !== request,
    ),
  });
}
