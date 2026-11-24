import { confirm } from "@tauri-apps/plugin-dialog";
import { create } from "zustand";
import { api } from "@/lib/tauri";
import type { RepositoryPublishScope } from "@/bindings";
import {
  applyPathParamRowChanges,
  buildQueryString,
  derivePathParamRows,
  deriveQueryRows,
  loadQuery,
  replaceQueryInUrl,
} from "@/features/requests/urlParams";
import {
  findNodeById,
  findRequestNode,
  insertNodeAt,
  moveNodeInTree,
  placementAfterRequest,
  removeNodeById,
  renameFolderNode,
  requestIdsForNode,
} from "@/features/collections/tree";
import { flushVariableEdits } from "@/features/variables/autosave";
import { getDraft, useDraftStore } from "@/features/workspace/draftStore";
import type { RequestTab } from "@/features/workspace/repositoryTabs";
import { mayDiscardDrafts } from "./discardDrafts";
import { useResponseStore } from "@/features/workspace/responseStore";
import {
  preferredEnvironmentIdFor,
  getWorkspaceUiState,
  useWorkspaceUiStore,
} from "@/features/workspace/workspaceUiStore";
import type {
  CollectionNode,
  CollectionSummary,
  EnvironmentSummary,
  ImportedCollection,
  RequestDetail,
} from "@/features/types";

let repositoryRefreshInFlight: Promise<void> | undefined;
let previewGeneration = 0;

export function waitForRepositoryRefresh() {
  return repositoryRefreshInFlight;
}

type WorkspaceState = {
  collections: CollectionSummary[];
  environments: EnvironmentSummary[];
  repositoryCollectionVersions: ReadonlyMap<string, number>;
  tree: CollectionNode[];
  tabs: RequestTab[];
  activeCollectionId?: string;
  activeRequestId?: string;
  sidebarVisible: boolean;
  // The sidebar starts open so there is something to pick, then hides once on
  // the first opened request. After that, or after any manual toggle, it stays
  // where the user puts it.
  sidebarAutoHidePending: boolean;
  saving: boolean;
  sendingRequestIds: ReadonlySet<string>;
  // False until the first `loadCollections` finishes, even if it failed. The
  // perf harness counts startup until then.
  collectionsLoaded: boolean;
  error?: string;
  // A folder made by ⌘⇧N, for the sidebar to open in rename once it shows.
  pendingRenameId?: string;
  loadCollections: () => Promise<void>;
  refreshRepositories: () => Promise<void>;
  openRepository: (
    root: string,
    create: boolean,
    source: string | null,
    preview?: RepositoryPublishScope[],
  ) => Promise<boolean>;
  relinkRepository: (collectionId: string, root: string) => Promise<void>;
  loadEnvironments: () => Promise<void>;
  importCollection: (path: string) => Promise<void>;
  // Undefined when the user cancels. Rejects instead of setting `error`, so
  // each caller shows the failure where the user is looking.
  pickAndImportCollection: () => Promise<CollectionSummary | undefined>;
  // The new collection's id, or undefined when it could not be created.
  createCollection: (name: string) => Promise<string | undefined>;
  deleteCollection: (collectionId: string) => Promise<void>;
  renameCollection: (collectionId: string, name: string) => Promise<void>;
  importEnvironment: (
    collectionId: string,
    contents: string,
    fileName: string,
  ) => Promise<void>;
  selectCollection: (collectionId: string) => Promise<void>;
  selectEnvironment: (
    collectionId: string,
    environmentId: string | null,
  ) => Promise<void>;
  selectRequest: (requestId: string) => Promise<void>;
  closeRequestTab: (requestId: string) => Promise<void>;
  createRequestIn: (parentId: string | null, position: number) => Promise<void>;
  // Resolves to the new folder's node id, so the sidebar can start renaming it.
  createFolderIn: (
    parentId: string | null,
    position: number,
  ) => Promise<string | undefined>;
  renameFolder: (nodeId: string, name: string) => Promise<void>;
  createRequestNextToActive: () => Promise<void>;
  createFolderNextToActive: () => Promise<void>;
  clearPendingRename: () => void;
  duplicateRequest: (requestId: string) => Promise<void>;
  deleteNode: (node: CollectionNode) => Promise<void>;
  moveNode: (
    node: CollectionNode,
    parentId: string | null,
    position: number,
  ) => Promise<void>;
  updateRequest: (patch: Partial<RequestDetail>) => void;
  saveActiveRequest: () => Promise<void>;
  resolveActiveRequest: () => Promise<void>;
  sendActiveRequest: () => Promise<void>;
  /** Resolves `true` once the command is on the clipboard. */
  copyActiveRequestAsCurl: (includeValues: boolean) => Promise<boolean>;
  toggleSidebar: () => void;
  setError: (error: unknown) => void;
  clearError: () => void;
};

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  collections: [],
  environments: [],
  repositoryCollectionVersions: new Map(),
  tree: [],
  tabs: [],
  sidebarVisible: true,
  sidebarAutoHidePending: true,
  saving: false,
  sendingRequestIds: new Set(),
  collectionsLoaded: false,

  loadCollections: async () => {
    set({ error: undefined });
    try {
      // Independent round trips on the startup path, so they run together.
      const [collections, environments, workspaceUi] = await Promise.all([
        api.listCollections(),
        api.listEnvironments(),
        useWorkspaceUiStore.getState().loadWorkspaceUiState(),
      ]);
      set({ collections, environments });
      const preferredCollectionId =
        get().activeCollectionId ?? workspaceUi.activeCollectionId;
      const current = collections.some(
        (collection) => collection.id === preferredCollectionId,
      )
        ? preferredCollectionId
        : collections[0]?.id;
      if (current) {
        await get().selectCollection(current);
      }
    } catch (error) {
      set({ error: String(error) });
    }
    set({ collectionsLoaded: true });
  },

  refreshRepositories: async () => {
    if (repositoryRefreshInFlight) return repositoryRefreshInFlight;
    const linked = get().collections.filter(
      (collection) => collection.repository,
    );
    if (!linked.length) return;
    repositoryRefreshInFlight = api
      .refreshRepositories(linked)
      .then(async (refresh) => {
        const { applyRepositoryRefresh } =
          await import("./repositoryWorkspace");
        await applyRepositoryRefresh(refresh);
      })
      .catch((error) => set({ error: String(error) }))
      .finally(() => {
        repositoryRefreshInFlight = undefined;
      });
    await repositoryRefreshInFlight;
  },

  openRepository: async (root, create, source, preview) => {
    const { openRepository } = await import("./repositoryOpen");
    return openRepository(root, create, source, preview);
  },

  relinkRepository: async (collectionId, root) => {
    await repositoryRefreshInFlight;
    set({ collections: await api.relinkRepository(collectionId, root) });
    await get().refreshRepositories();
  },

  loadEnvironments: async () => {
    try {
      const environments = await api.listEnvironments();
      const { activeCollectionId } = get();
      const previous =
        activeCollectionId && activeEnvironmentIdFor(activeCollectionId);
      set({ environments });
      if (
        activeCollectionId &&
        previous !== activeEnvironmentIdFor(activeCollectionId)
      ) {
        useDraftStore.getState().clearCollectionPreviews(activeCollectionId);
      }
    } catch (error) {
      set({ error: String(error) });
    }
  },

  importCollection: async (path) => {
    const approved = await mayDiscardDrafts(get);
    if (!approved) return;
    set({ error: undefined });
    try {
      await showCollection(await api.importPostmanCollection(path), approved);
    } catch (error) {
      set({ error: String(error) });
    }
  },

  pickAndImportCollection: async () => {
    const approved = await mayDiscardDrafts(get);
    if (!approved) return undefined;
    const imported = await api.pickAndImportPostmanCollection();
    if (!imported) return undefined;
    await showCollection(imported, approved);
    return imported.collections.find(
      (collection) => collection.id === imported.collectionId,
    );
  },

  createCollection: async (name) => {
    const approved = await mayDiscardDrafts(get);
    if (!approved) return undefined;
    set({ error: undefined });
    try {
      const created = await api.createCollection(name);
      await showCollection(created, approved);
      return created.collectionId;
    } catch (error) {
      set({ error: String(error) });
      return undefined;
    }
  },

  deleteCollection: async (collectionId) => {
    set({ error: undefined });
    try {
      await repositoryRefreshInFlight;
      const collections = await api.deleteCollection(collectionId);
      set({ collections });
      useWorkspaceUiStore.getState().setActiveEnvironmentId(collectionId, null);
      // Its environments went with it.
      await get().loadEnvironments();
      if (get().activeCollectionId !== collectionId) return;
      const next = collections[0]?.id;
      set({
        tree: [],
        sidebarVisible: next ? get().sidebarVisible : true,
        activeCollectionId: undefined,
        activeRequestId: undefined,
        tabs: [],
      });
      useDraftStore.getState().clearAll();
      useResponseStore.getState().clearAll();
      if (next) await get().selectCollection(next);
    } catch (error) {
      set({ error: String(error) });
    }
  },

  renameCollection: async (collectionId, name) => {
    set({ error: undefined });
    try {
      set({ collections: await api.renameCollection(collectionId, name) });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  importEnvironment: async (collectionId, contents, fileName) => {
    set({ error: undefined });
    try {
      const environmentId = await api.importEnvironment(
        collectionId,
        contents,
        fileName,
      );
      const environments = await api.listEnvironments();
      set({ environments });
      await get().selectEnvironment(collectionId, environmentId);
      await useWorkspaceUiStore.getState().flushWorkspaceUiState();
    } catch (error) {
      set({ error: String(error) });
    }
  },

  selectCollection: async (collectionId) => {
    const approved = await mayDiscardDrafts(get);
    if (!approved) return;
    set({ error: undefined });
    try {
      const tree = await api.getCollectionTree(collectionId);
      await showCollection(
        { collectionId, collections: get().collections, tree },
        approved,
      );
    } catch (error) {
      set({ error: String(error) });
    }
  },

  selectEnvironment: async (collectionId, environmentId) => {
    useWorkspaceUiStore
      .getState()
      .setActiveEnvironmentId(collectionId, environmentId);
    useDraftStore.getState().clearCollectionPreviews(collectionId);
    if (get().activeRequestId && get().activeCollectionId === collectionId) {
      await get().resolveActiveRequest();
    }
  },

  selectRequest: async (requestId) => {
    if (get().sidebarAutoHidePending) {
      set({ sidebarVisible: false, sidebarAutoHidePending: false });
    }
    const existingDraft = getDraft(requestId);
    if (existingDraft) {
      set({ activeRequestId: requestId });
      await get().resolveActiveRequest();
      return;
    }

    const collectionId = get().activeCollectionId;
    set({ activeRequestId: requestId, error: undefined });
    try {
      const request = normalizeRequestParams(await api.getRequest(requestId));
      if (get().activeCollectionId !== collectionId || getDraft(requestId))
        return;
      useDraftStore.getState().setDraft(request);
      set((state) => ({
        tabs: upsertTab(state.tabs, request, false),
      }));
      await get().resolveActiveRequest();
    } catch (error) {
      if (get().activeRequestId === requestId) {
        set({ error: String(error) });
      }
    }
  },

  closeRequestTab: async (requestId) => {
    const tab = get().tabs.find((item) => item.requestId === requestId);
    if (
      tab?.dirty &&
      getDraft(requestId)?.repositoryRevision &&
      !(await confirm("Discard the unsaved request draft?", {
        kind: "warning",
      }))
    )
      return;
    await closeRequestTab(requestId);
  },

  createRequestIn: async (parentId, position) => {
    const collectionId = get().activeCollectionId;
    if (!collectionId) return;
    set({ error: undefined });
    try {
      const result = await api.createRequest(collectionId, parentId, position);
      const node: CollectionNode = {
        id: result.nodeId,
        parentId,
        position,
        kind: "request",
        name: result.name,
        requestId: result.requestId,
        method: "GET",
      };
      set((state) => ({
        tree: insertNodeAt(state.tree, parentId, position, node),
      }));
      await get().selectRequest(result.requestId);
    } catch (error) {
      set({ error: String(error) });
    }
  },

  createFolderIn: async (parentId, position) => {
    const collectionId = get().activeCollectionId;
    if (!collectionId) return;
    set({ error: undefined });
    try {
      const { nodeId, name } = await api.createFolder(
        collectionId,
        parentId,
        position,
      );
      const node: CollectionNode = {
        id: nodeId,
        parentId,
        position,
        kind: "folder",
        name,
        children: [],
      };
      set((state) => ({
        tree: insertNodeAt(state.tree, parentId, position, node),
      }));
      return nodeId;
    } catch (error) {
      set({ error: String(error) });
    }
  },

  renameFolder: async (nodeId, name) => {
    set({ error: undefined });
    try {
      await api.renameFolder(nodeId, name);
      set((state) => ({ tree: renameFolderNode(state.tree, nodeId, name) }));
    } catch (error) {
      set({ error: String(error) });
    }
  },

  // New requests go right after the active one, or at the end of the root.
  createRequestNextToActive: async () => {
    const { tree, activeRequestId } = get();
    const { parentId, position } = placementAfterRequest(tree, activeRequestId);
    await get().createRequestIn(parentId, position);
  },

  // Placed like a new request; the sidebar is shown so the rename is visible.
  createFolderNextToActive: async () => {
    const { tree, activeRequestId } = get();
    const { parentId, position } = placementAfterRequest(tree, activeRequestId);
    const nodeId = await get().createFolderIn(parentId, position);
    if (nodeId) {
      set({
        pendingRenameId: nodeId,
        sidebarVisible: true,
        sidebarAutoHidePending: false,
      });
    }
  },

  clearPendingRename: () => set({ pendingRenameId: undefined }),

  duplicateRequest: async (requestId) => {
    const collectionId = get().activeCollectionId;
    if (!collectionId) return;
    set({ error: undefined });
    const sourceNode = findRequestNode(get().tree, requestId);
    try {
      const result = await api.duplicateRequest(requestId);
      if (sourceNode) {
        const node: CollectionNode = {
          ...sourceNode,
          id: result.nodeId,
          position: sourceNode.position + 1,
          name: `${sourceNode.name} Copy`,
          requestId: result.requestId,
        };
        set((state) => ({
          tree: insertNodeAt(
            state.tree,
            sourceNode.parentId,
            sourceNode.position + 1,
            node,
          ),
        }));
      } else {
        const tree = await api.getCollectionTree(collectionId);
        set({ tree });
      }
      await get().selectRequest(result.requestId);
    } catch (error) {
      set({ error: String(error) });
    }
  },

  deleteNode: async (node) => {
    set({ error: undefined });
    try {
      await api.deleteNode(node.id);
      const removedRequestIds = requestIdsForNode(node);
      const current = get();
      const activeDeleted =
        current.activeRequestId !== undefined &&
        removedRequestIds.includes(current.activeRequestId);
      useDraftStore.getState().removeMany(removedRequestIds);
      useResponseStore.getState().removeMany(removedRequestIds);
      set({
        tree: removeNodeById(current.tree, node.id),
        tabs: current.tabs.filter(
          (tab) => !removedRequestIds.includes(tab.requestId),
        ),
        activeRequestId: activeDeleted ? undefined : current.activeRequestId,
      });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  moveNode: async (node, parentId, position) => {
    set({ error: undefined });
    try {
      await api.moveNode(node.id, parentId, position);
      set((state) => {
        const current = findNodeById(state.tree, node.id);
        return current
          ? { tree: moveNodeInTree(state.tree, current, parentId, position) }
          : state;
      });
      // The new parent may change the request's inherited auth.
      await get().resolveActiveRequest();
    } catch (error) {
      set({ error: String(error) });
    }
  },

  updateRequest: (patch) => {
    const requestId = get().activeRequestId;
    const current = getDraft(requestId);
    if (!current) return;
    let request: RequestDetail = { ...current, ...patch };

    if (patch.url !== undefined && patch.url !== current.url) {
      request = {
        ...request,
        query: deriveQueryRows(request.url, current.query),
        pathParams: derivePathParamRows(request.url, current.pathParams),
      };
    } else if (patch.query !== undefined) {
      const nextUrl = replaceQueryInUrl(
        current.url,
        buildQueryString(patch.query),
      );
      request = { ...request, url: nextUrl };
    } else if (patch.pathParams !== undefined) {
      const nextUrl = applyPathParamRowChanges(
        current.url,
        current.pathParams,
        patch.pathParams,
      );
      request = {
        ...request,
        url: nextUrl,
        pathParams: derivePathParamRows(nextUrl, patch.pathParams),
      };
    }

    useDraftStore.getState().setDraft(request);
    set((state) => ({
      tabs: upsertTab(state.tabs, request, true, { keepConflict: true }),
    }));
  },

  saveActiveRequest: async () => {
    // Saving covers everything on screen, so a variable edit still inside
    // its autosave pause lands too.
    await flushVariableEdits();
    const { activeRequestId } = get();
    if (!activeRequestId || get().saving) return;
    const { saveDraft } = await import("./saveDraft");
    const current = get();
    const request = getDraft(activeRequestId);
    if (
      !request ||
      current.activeRequestId !== activeRequestId ||
      current.saving
    )
      return;
    const tab = current.tabs.find((item) => item.requestId === request.id);
    if (request.repositoryRevision && (!tab?.dirty || tab.conflict)) return;
    await saveDraft(request);
  },

  resolveActiveRequest: async () => {
    const generation = ++previewGeneration;
    const state = get();
    const request = getDraft(state.activeRequestId);
    if (!request) return;
    if (isRemovedDraft(state, request.id)) {
      useDraftStore.getState().clearPreview(request.id);
      return;
    }
    const environmentId = activeEnvironmentIdFor(request.collectionId);
    const isCurrent = () =>
      generation === previewGeneration &&
      get().activeRequestId === request.id &&
      getDraft(request.id)?.repositoryRevision === request.repositoryRevision &&
      activeEnvironmentIdFor(request.collectionId) === environmentId &&
      !isRemovedDraft(get(), request.id);
    try {
      const resolvedPreview = await api.resolveRequest(request, environmentId);
      if (isCurrent()) {
        useDraftStore.getState().setPreview(request.id, resolvedPreview);
      }
    } catch (error) {
      if (isCurrent()) set({ error: String(error) });
    }
  },

  copyActiveRequestAsCurl: async (includeValues) => {
    const state = get();
    const request = getDraft(state.activeRequestId);
    if (!request || (includeValues && isRemovedDraft(state, request.id))) {
      return false;
    }
    try {
      await flushVariableEdits();
      const curl = await api.requestAsCurl(
        request,
        activeEnvironmentIdFor(request.collectionId),
        includeValues,
      );
      if (includeValues && isRemovedDraft(get(), request.id)) return false;
      await navigator.clipboard.writeText(curl);
      return true;
    } catch (error) {
      set({ error: String(error) });
      return false;
    }
  },

  sendActiveRequest: async () => {
    const state = get();
    const request = getDraft(state.activeRequestId);
    if (
      !request ||
      state.sendingRequestIds.has(request.id) ||
      isRemovedDraft(state, request.id)
    )
      return;
    set((state) => ({
      sendingRequestIds: new Set(state.sendingRequestIds).add(request.id),
      error: undefined,
    }));
    const finishSending = () =>
      set((state) => {
        const sendingRequestIds = new Set(state.sendingRequestIds);
        sendingRequestIds.delete(request.id);
        return { sendingRequestIds };
      });
    try {
      // Rust reads variables from the database, so one typed a moment ago
      // must be saved first.
      await flushVariableEdits();
      const response = await api.sendRequest(
        request,
        activeEnvironmentIdFor(request.collectionId),
      );
      useResponseStore.getState().setResponse(request.id, response);
      finishSending();
    } catch (error) {
      finishSending();
      if (get().activeRequestId === request.id) set({ error: String(error) });
    }
    await get().resolveActiveRequest();
  },

  toggleSidebar: () =>
    set((state) => ({
      sidebarVisible: !state.sidebarVisible,
      sidebarAutoHidePending: false,
    })),
  setError: (error) => set({ error: String(error) }),
  clearError: () => set({ error: undefined }),
}));

function isRemovedDraft(state: WorkspaceState, requestId: string) {
  return (
    state.tabs.find((tab) => tab.requestId === requestId)?.conflict
      ?.revision === null
  );
}

function availableEnvironment(
  environments: EnvironmentSummary[],
  collectionId: string | undefined,
  preferredId: string | null,
) {
  if (!preferredId) return undefined;
  return environments.find(
    (environment) =>
      environment.id === preferredId &&
      environment.collectionId === collectionId,
  );
}

export function activeEnvironmentIdFor(collectionId: string) {
  return (
    availableEnvironment(
      useWorkspaceStore.getState().environments,
      collectionId,
      preferredEnvironmentIdFor(getWorkspaceUiState(), collectionId),
    )?.id ?? null
  );
}

export function useActiveEnvironment(collectionId: string | undefined) {
  const preferredId = useWorkspaceUiStore((state) =>
    preferredEnvironmentIdFor(state.workspaceUi, collectionId),
  );
  const environments = useWorkspaceStore((state) => state.environments);
  return availableEnvironment(environments, collectionId, preferredId);
}

export async function showCollection(
  { collectionId, collections, tree }: ImportedCollection,
  approved: () => boolean | Promise<boolean>,
) {
  if (!(await approved())) {
    useWorkspaceStore.setState({ collections });
    return false;
  }
  useWorkspaceStore.setState({
    collections,
    tree,
    activeCollectionId: collectionId,
    activeRequestId: undefined,
    tabs: [],
  });
  useWorkspaceUiStore.getState().setActiveCollectionId(collectionId);
  useDraftStore.getState().clearAll();
  useResponseStore.getState().clearAll();
  await useWorkspaceUiStore.getState().flushWorkspaceUiState();
  return true;
}

export async function closeRequestTab(requestId: string) {
  const current = useWorkspaceStore.getState();
  const tabIndex = current.tabs.findIndex((tab) => tab.requestId === requestId);
  const tabs = current.tabs.filter((tab) => tab.requestId !== requestId);
  useDraftStore.getState().removeMany([requestId]);
  useResponseStore.getState().removeMany([requestId]);
  const active = current.activeRequestId === requestId;
  useWorkspaceStore.setState({
    tabs,
    activeRequestId: active ? undefined : current.activeRequestId,
  });
  const next = tabs[Math.max(0, tabIndex - 1)];
  if (active && next) await current.selectRequest(next.requestId);
}

export function normalizeRequestParams(request: RequestDetail): RequestDetail {
  const { url, query } = loadQuery(request.url, request.query);
  return {
    ...request,
    url,
    query,
    pathParams: derivePathParamRows(url, request.pathParams),
  };
}

export function upsertTab(
  tabs: RequestTab[],
  request: RequestDetail,
  dirty: boolean,
  { keepConflict = false }: { keepConflict?: boolean } = {},
) {
  const tab = {
    requestId: request.id,
    name: request.name,
    method: request.method,
    dirty,
  };
  const index = tabs.findIndex((item) => item.requestId === request.id);
  if (index === -1) return [...tabs, tab];
  const current = tabs[index];
  const next = {
    ...tab,
    conflict: keepConflict ? current.conflict : undefined,
  };
  // Same array when nothing shown changed, so typing does not re-render the
  // tab bar on every keystroke.
  if (
    current.name === next.name &&
    current.method === next.method &&
    current.dirty === next.dirty &&
    current.conflict === next.conflict
  ) {
    return tabs;
  }
  return tabs.map((item, i) => (i === index ? next : item));
}
