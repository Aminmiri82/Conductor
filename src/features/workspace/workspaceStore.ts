import { create } from "zustand";
import { api } from "@/lib/tauri";
import {
  applyPathParamRowChanges,
  buildQueryString,
  derivePathParamRows,
  deriveQueryRows,
  loadQuery,
  replaceQueryInUrl,
} from "@/features/requests/urlParams";
import {
  findRequestNode,
  insertNodeAt,
  moveNodeInTree,
  placementAfterRequest,
  removeNodeById,
  requestIdsForNode,
  updateRequestNode,
} from "@/features/collections/tree";
import { getDraft, useDraftStore } from "@/features/workspace/draftStore";
import { useResponseStore } from "@/features/workspace/responseStore";
import {
  activeEnvironmentIdFor,
  getWorkspaceUiState,
  useWorkspaceUiStore,
} from "@/features/workspace/workspaceUiStore";
import type {
  CollectionNode,
  CollectionSummary,
  EnvironmentSummary,
  RequestDetail,
} from "@/features/types";

type RequestTab = {
  requestId: string;
  name: string;
  method: string;
  dirty: boolean;
};

type WorkspaceState = {
  collections: CollectionSummary[];
  environments: EnvironmentSummary[];
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
  error?: string;
  loadCollections: () => Promise<void>;
  loadEnvironments: () => Promise<void>;
  importCollection: (json: string) => Promise<void>;
  importEnvironment: (
    collectionId: string,
    contents: string,
    fileName?: string | null,
  ) => Promise<void>;
  selectCollection: (collectionId: string) => Promise<void>;
  selectEnvironment: (
    collectionId: string,
    environmentId: string | null,
  ) => Promise<void>;
  selectRequest: (requestId: string) => Promise<void>;
  closeRequestTab: (requestId: string) => Promise<void>;
  createRequestIn: (
    parentId: string | null | undefined,
    position: number,
  ) => Promise<void>;
  createFolderIn: (
    parentId: string | null | undefined,
    position: number,
  ) => Promise<void>;
  createRequestNextToActive: () => Promise<void>;
  duplicateRequest: (requestId: string) => Promise<void>;
  deleteNode: (node: CollectionNode) => Promise<void>;
  moveNode: (
    node: CollectionNode,
    parentId: string | null | undefined,
    position: number,
  ) => Promise<void>;
  updateRequest: (patch: Partial<RequestDetail>) => void;
  saveActiveRequest: () => Promise<void>;
  resolveActiveRequest: () => Promise<void>;
  sendActiveRequest: () => Promise<void>;
  toggleSidebar: () => void;
  setError: (error: unknown) => void;
  clearError: () => void;
};

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  collections: [],
  environments: [],
  tree: [],
  tabs: [],
  sidebarVisible: true,
  sidebarAutoHidePending: true,
  saving: false,
  sendingRequestIds: new Set(),

  loadCollections: async () => {
    set({ error: undefined });
    try {
      // Independent round trips on the startup path, so they run together.
      const environmentsPromise = api.listEnvironments();
      const [collections, environments, workspaceUi] = await Promise.all([
        api.listCollections(),
        environmentsPromise,
        useWorkspaceUiStore
          .getState()
          .loadWorkspaceUiState(environmentsPromise),
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
  },

  loadEnvironments: async () => {
    try {
      const environments = await api.listEnvironments();
      useWorkspaceUiStore.getState().reconcileActiveEnvironments(environments);
      set({ environments });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  importCollection: async (json) => {
    set({ error: undefined });
    try {
      const collectionId = await api.importPostmanCollection(json);
      const collections = await api.listCollections();
      const tree = await api.getCollectionTree(collectionId);
      set({
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
      useWorkspaceUiStore
        .getState()
        .setActiveEnvironmentId(collectionId, environmentId);
      await useWorkspaceUiStore.getState().flushWorkspaceUiState();
      await get().resolveActiveRequest();
    } catch (error) {
      set({ error: String(error) });
    }
  },

  selectCollection: async (collectionId) => {
    set({ error: undefined });
    try {
      const tree = await api.getCollectionTree(collectionId);
      set({
        tree,
        activeCollectionId: collectionId,
        activeRequestId: undefined,
        tabs: [],
      });
      useWorkspaceUiStore.getState().setActiveCollectionId(collectionId);
      useDraftStore.getState().clearAll();
      useResponseStore.getState().clearAll();
      await useWorkspaceUiStore.getState().flushWorkspaceUiState();
    } catch (error) {
      set({ error: String(error) });
    }
  },

  selectEnvironment: async (collectionId, environmentId) => {
    useWorkspaceUiStore
      .getState()
      .setActiveEnvironmentId(collectionId, environmentId);
    await get().resolveActiveRequest();
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

    set({ activeRequestId: requestId, error: undefined });
    try {
      const request = normalizeRequestParams(await api.getRequest(requestId));
      useDraftStore.getState().setDraft(request);
      set((state) => ({
        tabs: upsertTab(state.tabs, request, false, false),
      }));
      await get().resolveActiveRequest();
    } catch (error) {
      if (get().activeRequestId === requestId) {
        set({ error: String(error) });
      }
    }
  },

  closeRequestTab: async (requestId) => {
    const current = get();
    const tabIndex = current.tabs.findIndex(
      (tab) => tab.requestId === requestId,
    );
    const tabs = current.tabs.filter((tab) => tab.requestId !== requestId);
    useDraftStore.getState().removeMany([requestId]);
    useResponseStore.getState().removeMany([requestId]);
    if (current.activeRequestId !== requestId) {
      set({ tabs });
      return;
    }

    const nextTab = tabs[Math.max(0, tabIndex - 1)] ?? tabs[0];
    set({ tabs, activeRequestId: undefined });
    if (nextTab) {
      await get().selectRequest(nextTab.requestId);
    }
  },

  createRequestIn: async (parentId, position) => {
    const collectionId = get().activeCollectionId;
    if (!collectionId) return;
    const name = "New Request";
    set({ error: undefined });
    try {
      const result = await api.createRequest(
        collectionId,
        parentId,
        position,
        name,
      );
      const node: CollectionNode = {
        id: result.nodeId,
        parentId: parentId ?? null,
        position,
        kind: "request",
        name,
        requestId: result.requestId,
        method: "GET",
      };
      set((state) => ({
        tree: insertNodeAt(state.tree, parentId ?? null, position, node),
      }));
      await get().selectRequest(result.requestId);
    } catch (error) {
      set({ error: String(error) });
    }
  },

  createFolderIn: async (parentId, position) => {
    const collectionId = get().activeCollectionId;
    if (!collectionId) return;
    const name = "New Folder";
    set({ error: undefined });
    try {
      const nodeId = await api.createFolder(
        collectionId,
        parentId,
        position,
        name,
      );
      const node: CollectionNode = {
        id: nodeId,
        parentId: parentId ?? null,
        position,
        kind: "folder",
        name,
        children: [],
      };
      set((state) => ({
        tree: insertNodeAt(state.tree, parentId ?? null, position, node),
      }));
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
            sourceNode.parentId ?? null,
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
      set((state) => ({
        tree: moveNodeInTree(state.tree, node, parentId ?? null, position),
      }));
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
      tabs: upsertTab(state.tabs, request, true, true),
    }));
  },

  saveActiveRequest: async () => {
    const request = getDraft(get().activeRequestId);
    if (!request) return;
    set({ error: undefined, saving: true });
    try {
      await api.saveRequest(request);
      set((state) => ({
        saving: false,
        tabs: upsertTab(state.tabs, request, false, true),
        tree: updateRequestNode(state.tree, request),
      }));
      await get().resolveActiveRequest();
    } catch (error) {
      set({
        error:
          get().activeRequestId === request.id ? String(error) : get().error,
        saving: false,
      });
    }
  },

  resolveActiveRequest: async () => {
    const request = getDraft(get().activeRequestId);
    if (!request) return;
    try {
      const resolvedPreview = await api.resolveRequest(
        request,
        activeEnvironmentIdFor(getWorkspaceUiState(), request.collectionId),
      );
      if (get().activeRequestId === request.id) {
        useDraftStore.getState().setPreview(request.id, resolvedPreview);
      }
    } catch (error) {
      if (get().activeRequestId === request.id) {
        set({ error: String(error) });
      }
    }
  },

  sendActiveRequest: async () => {
    const request = getDraft(get().activeRequestId);
    if (!request || get().sendingRequestIds.has(request.id)) return;
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
      const response = await api.sendRequest(
        request,
        activeEnvironmentIdFor(getWorkspaceUiState(), request.collectionId),
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

function normalizeRequestParams(request: RequestDetail): RequestDetail {
  const { url, query } = loadQuery(request.url, request.query);
  return {
    ...request,
    url,
    query,
    pathParams: derivePathParamRows(url, request.pathParams),
  };
}

function upsertTab(
  tabs: RequestTab[],
  request: RequestDetail,
  dirty: boolean,
  replaceDirty: boolean,
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
  const next = { ...tab, dirty: replaceDirty ? dirty : current.dirty };
  // Same array when nothing shown changed, so typing does not re-render the
  // tab bar on every keystroke.
  if (
    current.name === next.name &&
    current.method === next.method &&
    current.dirty === next.dirty
  ) {
    return tabs;
  }
  return tabs.map((item, i) => (i === index ? next : item));
}
