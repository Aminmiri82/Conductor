import type { SaveRequestResult } from "@/bindings";
import type { RepositoryRefresh, RequestDetail } from "@/features/types";
import { api } from "@/lib/tauri";
import { getDraft, useDraftStore } from "./draftStore";
import { reconcileRepositoryTabs } from "./repositoryTabs";
import { useResponseStore } from "./responseStore";
import {
  activeEnvironmentIdFor,
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
  changedEnvironmentIds,
  changedVariableCollectionIds,
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
  const collectionId = initial.activeCollectionId;
  const environmentId = collectionId && activeEnvironmentIdFor(collectionId);
  if (changedEnvironmentIds.length) await initial.loadEnvironments();
  const variableChanges = new Set([
    ...changedCollectionIds,
    ...changedVariableCollectionIds,
  ]);
  if (variableChanges.size) {
    useWorkspaceStore.setState((state) => {
      const versions = new Map(state.repositoryCollectionVersions);
      for (const id of variableChanges) {
        versions.set(id, (versions.get(id) ?? 0) + 1);
      }
      return { repositoryCollectionVersions: versions };
    });
  }
  if (!collectionId) return;
  const treeChanged = changedCollectionIds.includes(collectionId);
  const selectedEnvironmentChanged =
    environmentId !== activeEnvironmentIdFor(collectionId);
  if (variableChanges.has(collectionId)) {
    useDraftStore.getState().clearCollectionPreviews(collectionId);
  }
  if (!treeChanged) {
    if (
      selectedEnvironmentChanged ||
      variableChanges.has(collectionId) ||
      (environmentId && changedEnvironmentIds.includes(environmentId))
    )
      await useWorkspaceStore.getState().resolveActiveRequest();
    return;
  }

  let current = useWorkspaceStore.getState();
  let tree;
  while (current.activeCollectionId === collectionId) {
    const previous = current.tree;
    tree = await api.getCollectionTree(collectionId);
    current = useWorkspaceStore.getState();
    // Retry if a local edit made the tree read stale while it was in flight.
    if (current.tree === previous) break;
  }
  if (current.activeCollectionId !== collectionId) return;

  const { tabs, invalidate, closed } = reconcileRepositoryTabs(
    current.tabs,
    changedRequests,
    (requestId) => getDraft(requestId)?.repositoryRevision,
    tree,
  );
  useDraftStore.getState().removeMany(invalidate);
  useResponseStore.getState().removeMany(closed);
  useWorkspaceStore.setState({
    tabs,
    ...(tree ? { tree } : {}),
  });
  const active = current.activeRequestId;
  if (!active) return;
  const activeTab = tabs.find((tab) => tab.requestId === active);
  if (invalidate.includes(active)) {
    const next = activeTab ?? tabs[0];
    if (next) await current.selectRequest(next.requestId);
    else useWorkspaceStore.setState({ activeRequestId: undefined });
  } else {
    await current.resolveActiveRequest();
  }
}

export async function applyUnsuccessfulSave(
  requestId: string,
  result: Exclude<SaveRequestResult, { status: "saved" }>,
) {
  if (result.refresh) await applyRepositoryRefresh(result.refresh);
  if (result.status === "failed") {
    const state = useWorkspaceStore.getState();
    if (state.activeRequestId === requestId) state.setError(result.message);
    return;
  }
  if (result.revision === null)
    useDraftStore.getState().clearPreview(requestId);
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
