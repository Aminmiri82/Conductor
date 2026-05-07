import { create } from "zustand";
import { api } from "@/lib/tauri";
import type {
  CollectionNode,
  CollectionSummary,
  RequestDetail,
  ResolvedRequestPreview,
  SendRequestResult,
} from "@/features/types";

export type RequestTab = {
  requestId: string;
  name: string;
  method: string;
  dirty: boolean;
  request: RequestDetail;
};

type WorkspaceState = {
  collections: CollectionSummary[];
  tree: CollectionNode[];
  tabs: RequestTab[];
  activeCollectionId?: string;
  activeRequestId?: string;
  activeRequest?: RequestDetail;
  resolvedPreview?: ResolvedRequestPreview;
  response?: SendRequestResult;
  lastSavedAt?: number;
  sidebarVisible: boolean;
  loading: boolean;
  saving: boolean;
  sending: boolean;
  error?: string;
  loadCollections: () => Promise<void>;
  importCollection: (json: string) => Promise<void>;
  selectCollection: (collectionId: string) => Promise<void>;
  selectRequest: (requestId: string) => Promise<void>;
  closeRequestTab: (requestId: string) => Promise<void>;
  createRequestIn: (parentId: string | null | undefined, position: number) => Promise<void>;
  createFolderIn: (parentId: string | null | undefined, position: number) => Promise<void>;
  duplicateRequest: (requestId: string) => Promise<void>;
  deleteRequest: (requestId: string) => Promise<void>;
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
  clearError: () => void;
};

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  collections: [],
  tree: [],
  tabs: [],
  sidebarVisible: true,
  loading: false,
  saving: false,
  sending: false,

  loadCollections: async () => {
    set({ loading: true, error: undefined });
    try {
      const collections = await api.listCollections();
      set({ collections, loading: false });
      const current = get().activeCollectionId ?? collections[0]?.id;
      if (current) {
        await get().selectCollection(current);
      }
    } catch (error) {
      set({ error: String(error), loading: false });
    }
  },

  importCollection: async (json) => {
    set({ loading: true, error: undefined });
    try {
      const collectionId = await api.importPostmanCollection(json);
      const collections = await api.listCollections();
      const tree = await api.getCollectionTree(collectionId);
      set({
        collections,
        tree,
        activeCollectionId: collectionId,
        activeRequestId: undefined,
        activeRequest: undefined,
        tabs: [],
        response: undefined,
        resolvedPreview: undefined,
        loading: false,
      });
    } catch (error) {
      set({ error: String(error), loading: false });
    }
  },

  selectCollection: async (collectionId) => {
    set({ loading: true, error: undefined });
    try {
      const tree = await api.getCollectionTree(collectionId);
      set({
        tree,
        activeCollectionId: collectionId,
        activeRequestId: undefined,
        activeRequest: undefined,
        tabs: [],
        response: undefined,
        resolvedPreview: undefined,
        loading: false,
      });
    } catch (error) {
      set({ error: String(error), loading: false });
    }
  },

  selectRequest: async (requestId) => {
    const existingTab = get().tabs.find((tab) => tab.requestId === requestId);
    if (existingTab) {
      set({
        activeRequest: existingTab.request,
        activeRequestId: requestId,
        response: undefined,
        loading: false,
      });
      await get().resolveActiveRequest();
      return;
    }

    set({ loading: true, error: undefined });
    try {
      const activeRequest = await api.getRequest(requestId);
      const tabs = upsertTab(get().tabs, activeRequest, false, false);
      set({
        activeRequest,
        activeRequestId: requestId,
        tabs,
        response: undefined,
        loading: false,
      });
      await get().resolveActiveRequest();
    } catch (error) {
      set({ error: String(error), loading: false });
    }
  },

  closeRequestTab: async (requestId) => {
    const current = get();
    const tabIndex = current.tabs.findIndex((tab) => tab.requestId === requestId);
    const tabs = current.tabs.filter((tab) => tab.requestId !== requestId);
    if (current.activeRequestId !== requestId) {
      set({ tabs });
      return;
    }

    const nextTab = tabs[Math.max(0, tabIndex - 1)] ?? tabs[0];
    set({
      tabs,
      activeRequestId: undefined,
      activeRequest: undefined,
      response: undefined,
      resolvedPreview: undefined,
    });
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
      const requestId = await api.createRequest(
        collectionId,
        parentId,
        position,
        name,
      );
      const tree = await api.getCollectionTree(collectionId);
      set({ tree });
      await get().selectRequest(requestId);
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
      await api.createFolder(collectionId, parentId, position, name);
      const tree = await api.getCollectionTree(collectionId);
      set({ tree });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  duplicateRequest: async (requestId) => {
    const collectionId = get().activeCollectionId;
    if (!collectionId) return;
    set({ error: undefined });
    try {
      const duplicatedRequestId = await api.duplicateRequest(requestId);
      const tree = await api.getCollectionTree(collectionId);
      set({ tree });
      await get().selectRequest(duplicatedRequestId);
    } catch (error) {
      set({ error: String(error) });
    }
  },

  deleteRequest: async (requestId) => {
    const collectionId = get().activeCollectionId;
    if (!collectionId) return;
    const confirmed = window.confirm("Delete this request?");
    if (!confirmed) return;
    set({ error: undefined });
    try {
      await api.deleteRequest(requestId);
      const tree = await api.getCollectionTree(collectionId);
      const current = get();
      const tabs = current.tabs.filter((tab) => tab.requestId !== requestId);
      set({
        tree,
        tabs,
        activeRequestId:
          current.activeRequestId === requestId ? undefined : current.activeRequestId,
        activeRequest:
          current.activeRequestId === requestId ? undefined : current.activeRequest,
        response: current.activeRequestId === requestId ? undefined : current.response,
        resolvedPreview:
          current.activeRequestId === requestId ? undefined : current.resolvedPreview,
      });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  deleteNode: async (node) => {
    const collectionId = get().activeCollectionId;
    if (!collectionId) return;
    const confirmed = window.confirm(
      node.kind === "folder"
        ? `Delete folder "${node.name}" and everything inside it?`
        : `Delete request "${node.name}"?`,
    );
    if (!confirmed) return;
    set({ error: undefined });
    try {
      await api.deleteNode(node.id);
      const tree = await api.getCollectionTree(collectionId);
      const removedRequestIds = requestIdsForNode(node);
      const current = get();
      const activeDeleted =
        current.activeRequestId !== undefined &&
        removedRequestIds.includes(current.activeRequestId);
      set({
        tree,
        tabs: current.tabs.filter((tab) => !removedRequestIds.includes(tab.requestId)),
        activeRequestId: activeDeleted ? undefined : current.activeRequestId,
        activeRequest: activeDeleted ? undefined : current.activeRequest,
        response: activeDeleted ? undefined : current.response,
        resolvedPreview: activeDeleted ? undefined : current.resolvedPreview,
      });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  moveNode: async (node, parentId, position) => {
    const collectionId = get().activeCollectionId;
    if (!collectionId) return;
    set({ error: undefined });
    try {
      await api.moveNode(node.id, parentId, position);
      const tree = await api.getCollectionTree(collectionId);
      set({ tree });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  updateRequest: (patch) => {
    const current = get().activeRequest;
    if (!current) return;
    const activeRequest = { ...current, ...patch };
    set({
      activeRequest,
      tabs: upsertTab(get().tabs, activeRequest, true, true),
      lastSavedAt: undefined,
    });
  },

  saveActiveRequest: async () => {
    const request = get().activeRequest;
    if (!request) return;
    set({ error: undefined, saving: true });
    try {
      await api.saveRequest(request);
      set({
        saving: false,
        lastSavedAt: Date.now(),
        tabs: upsertTab(get().tabs, request, false, true),
        tree: renameRequestNode(get().tree, request.id, request.name),
      });
      await get().resolveActiveRequest();
    } catch (error) {
      set({ error: String(error), saving: false });
    }
  },

  resolveActiveRequest: async () => {
    const request = get().activeRequest;
    if (!request) return;
    try {
      const resolvedPreview = await api.resolveRequest(request);
      set({ resolvedPreview });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  sendActiveRequest: async () => {
    const request = get().activeRequest;
    if (!request) return;
    set({ sending: true, saving: true, error: undefined });
    try {
      await api.saveRequest(request);
      set({
        tabs: upsertTab(get().tabs, request, false, true),
        tree: renameRequestNode(get().tree, request.id, request.name),
        lastSavedAt: Date.now(),
        saving: false,
      });
      const response = await api.sendRequest(request);
      set({ response, sending: false });
      await get().resolveActiveRequest();
    } catch (error) {
      set({ error: String(error), sending: false, saving: false });
      await get().resolveActiveRequest();
    }
  },

  toggleSidebar: () => set((state) => ({ sidebarVisible: !state.sidebarVisible })),
  clearError: () => set({ error: undefined }),
}));

function renameRequestNode(
  nodes: CollectionNode[],
  requestId: string,
  name: string,
): CollectionNode[] {
  return nodes.map((node) => {
    if (node.requestId === requestId) {
      return { ...node, name };
    }
    if (node.children.length) {
      return { ...node, children: renameRequestNode(node.children, requestId, name) };
    }
    return node;
  });
}

function requestIdsForNode(node: CollectionNode): string[] {
  return [
    ...(node.requestId ? [node.requestId] : []),
    ...node.children.flatMap(requestIdsForNode),
  ];
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
    request,
  };
  const exists = tabs.some((item) => item.requestId === request.id);
  if (!exists) return [...tabs, tab];
  return tabs.map((item) =>
    item.requestId === request.id
      ? { ...item, ...tab, dirty: replaceDirty ? dirty : item.dirty }
      : item,
  );
}
