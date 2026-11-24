import type { RepositoryRefresh, RequestDetail } from "@/features/types";
import { api } from "@/lib/tauri";
import { getDraft, useDraftStore } from "./draftStore";
import { reconcileRepositoryTabs } from "./repositoryTabs";
import { useResponseStore } from "./responseStore";
import { saveDraft } from "./saveDraft";
import {
  activeEnvironmentIdFor,
  closeRequestTab,
  normalizeRequestParams,
  upsertTab,
  useWorkspaceStore,
} from "./workspaceStore";

type ConflictAction = "reload" | "keep" | "save-as-new";

let pendingTree: string | undefined;

export async function applyRepositoryRefresh({
  collections,
  changedRequests,
  changedCollectionIds,
  changedEnvironmentIds,
  changedVariableCollectionIds,
}: RepositoryRefresh) {
  const { getState: get, setState: set } = useWorkspaceStore;
  if (collections) set({ collections });
  const collectionId = get().activeCollectionId;
  const environmentId = collectionId && activeEnvironmentIdFor(collectionId);
  if (changedEnvironmentIds.length) await get().loadEnvironments();
  if (changedVariableCollectionIds.length) {
    set((state) => {
      const versions = new Map(state.repositoryCollectionVersions);
      for (const id of changedVariableCollectionIds) {
        versions.set(id, (versions.get(id) ?? 0) + 1);
      }
      return { repositoryCollectionVersions: versions };
    });
  }
  if (!collectionId || get().activeCollectionId !== collectionId) return;
  const treeChanged = changedCollectionIds.includes(collectionId);
  const variablesChanged = changedVariableCollectionIds.includes(collectionId);
  const selectedEnvironmentChanged =
    environmentId !== activeEnvironmentIdFor(collectionId);
  if (treeChanged || variablesChanged) {
    useDraftStore.getState().clearCollectionPreviews(collectionId);
  }

  let tree;
  if (treeChanged || pendingTree === collectionId) {
    pendingTree = collectionId;
    try {
      tree = await readUnchangedTree(collectionId);
      if (!tree && get().activeCollectionId === collectionId) {
        tree = await readUnchangedTree(collectionId);
      }
    } catch (error) {
      if (get().activeCollectionId === collectionId) get().setError(error);
    }
    if (get().activeCollectionId !== collectionId) return;
    // If both reads raced local edits, use the next refresh to catch up.
    pendingTree = tree ? undefined : collectionId;
  } else if (
    !variablesChanged &&
    !selectedEnvironmentChanged &&
    !(environmentId && changedEnvironmentIds.includes(environmentId))
  ) {
    return;
  }

  const current = get();
  const { tabs, invalidate, closed } = reconcileRepositoryTabs(
    current.tabs,
    changedRequests,
    (requestId) => getDraft(requestId)?.repositoryRevision,
  );
  useDraftStore.getState().removeMany(invalidate);
  useResponseStore.getState().removeMany(closed);
  set({ tabs, ...(tree ? { tree } : {}) });
  const active = current.activeRequestId;
  if (!active) return;
  const activeTab = tabs.find((tab) => tab.requestId === active);
  if (invalidate.includes(active)) {
    const next = activeTab ?? tabs[0];
    if (next) await current.selectRequest(next.requestId);
    else set({ activeRequestId: undefined });
  } else {
    await current.resolveActiveRequest();
  }
}

async function readUnchangedTree(collectionId: string) {
  const before = useWorkspaceStore.getState().tree;
  const tree = await api.getCollectionTree(collectionId);
  if (useWorkspaceStore.getState().tree === before) return tree;
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
