import { create } from "zustand";
import { api } from "@/lib/tauri";
import {
  applyPathParamRowChanges,
  buildQueryString,
  derivePathParamRows,
  deriveQueryRows,
  replaceQueryInUrl,
} from "@/features/requests/urlParams";
import { getDraft, useDraftStore } from "@/features/workspace/draftStore";
import { useResponseStore } from "@/features/workspace/responseStore";
import {
  getWorkspaceUiState,
  useWorkspaceUiStore,
} from "@/features/workspace/workspaceUiStore";
import type {
  CollectionNode,
  CollectionSummary,
  EnvironmentSummary,
  RequestDetail,
} from "@/features/types";

export type RequestTab = {
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
  collectionLoading: boolean;
  requestLoading: boolean;
  saving: boolean;
  sending: boolean;
  error?: string;
  loadCollections: () => Promise<void>;
  loadEnvironments: () => Promise<void>;
  importCollection: (json: string) => Promise<void>;
  importEnvironment: (
    contents: string,
    fileName?: string | null,
  ) => Promise<void>;
  selectCollection: (collectionId: string) => Promise<void>;
  selectEnvironment: (environmentId: string | null) => Promise<void>;
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
  environments: [],
  tree: [],
  tabs: [],
  sidebarVisible: true,
  collectionLoading: false,
  requestLoading: false,
  saving: false,
  sending: false,

  loadCollections: async () => {
    set({ collectionLoading: true, error: undefined });
    try {
      const collections = await api.listCollections();
      const environments = await api.listEnvironments();
      const workspaceUi = await useWorkspaceUiStore
        .getState()
        .loadWorkspaceUiState(environments);
      set({
        collections,
        environments,
        collectionLoading: false,
      });
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
      set({ error: String(error), collectionLoading: false });
    }
  },

  loadEnvironments: async () => {
    try {
      const environments = await api.listEnvironments();
      useWorkspaceUiStore.getState().reconcileActiveEnvironment(environments);
      set({ environments });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  importCollection: async (json) => {
    set({ collectionLoading: true, error: undefined });
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
        collectionLoading: false,
        requestLoading: false,
      });
      useWorkspaceUiStore.getState().setActiveCollectionId(collectionId);
      useDraftStore.getState().clearAll();
      useResponseStore.getState().clearAll();
      await useWorkspaceUiStore.getState().flushWorkspaceUiState();
    } catch (error) {
      set({ error: String(error), collectionLoading: false });
    }
  },

  importEnvironment: async (contents, fileName) => {
    set({ error: undefined });
    try {
      const environmentId = await api.importEnvironment(contents, fileName);
      const environments = await api.listEnvironments();
      set({ environments });
      useWorkspaceUiStore.getState().setActiveEnvironmentId(environmentId);
      await useWorkspaceUiStore.getState().flushWorkspaceUiState();
      await get().resolveActiveRequest();
    } catch (error) {
      set({ error: String(error) });
    }
  },

  selectCollection: async (collectionId) => {
    set({ collectionLoading: true, error: undefined });
    try {
      const tree = await api.getCollectionTree(collectionId);
      set({
        tree,
        activeCollectionId: collectionId,
        activeRequestId: undefined,
        tabs: [],
        collectionLoading: false,
        requestLoading: false,
      });
      useWorkspaceUiStore.getState().setActiveCollectionId(collectionId);
      useDraftStore.getState().clearAll();
      useResponseStore.getState().clearAll();
      await useWorkspaceUiStore.getState().flushWorkspaceUiState();
    } catch (error) {
      set({ error: String(error), collectionLoading: false });
    }
  },

  selectEnvironment: async (environmentId) => {
    useWorkspaceUiStore.getState().setActiveEnvironmentId(environmentId);
    await get().resolveActiveRequest();
  },

  selectRequest: async (requestId) => {
    const existingDraft = getDraft(requestId);
    if (existingDraft) {
      set({
        activeRequestId: requestId,
        requestLoading: false,
      });
      await get().resolveActiveRequest();
      return;
    }

    set({
      activeRequestId: requestId,
      requestLoading: true,
      error: undefined,
    });
    try {
      const request = normalizeRequestParams(await api.getRequest(requestId));
      useDraftStore.getState().setDraft(request, false);
      set((state) => ({
        tabs: upsertTab(state.tabs, request, false, false),
        requestLoading:
          state.activeRequestId === requestId ? false : state.requestLoading,
      }));
      await get().resolveActiveRequest();
    } catch (error) {
      if (get().activeRequestId === requestId) {
        set({ error: String(error), requestLoading: false });
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
    set({
      tabs,
      activeRequestId: undefined,
      requestLoading: false,
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
      const result = await api.createRequest(
        collectionId,
        parentId,
        position,
        name,
      );
      const node: CollectionNode = {
        id: result.nodeId,
        collectionId,
        parentId: parentId ?? null,
        position,
        kind: "request",
        name,
        requestId: result.requestId,
        method: "GET",
        children: [],
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
        collectionId,
        parentId: parentId ?? null,
        position,
        kind: "folder",
        name,
        requestId: null,
        method: null,
        children: [],
      };
      set((state) => ({
        tree: insertNodeAt(state.tree, parentId ?? null, position, node),
      }));
    } catch (error) {
      set({ error: String(error) });
    }
  },

  duplicateRequest: async (requestId) => {
    const collectionId = get().activeCollectionId;
    if (!collectionId) return;
    set({ error: undefined });
    const sourceNode = findRequestNodeByRequestId(get().tree, requestId);
    try {
      const result = await api.duplicateRequest(requestId);
      if (sourceNode) {
        const node: CollectionNode = {
          ...sourceNode,
          id: result.nodeId,
          position: sourceNode.position + 1,
          name: `${sourceNode.name} Copy`,
          requestId: result.requestId,
          children: [],
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

  deleteRequest: async (requestId) => {
    const collectionId = get().activeCollectionId;
    if (!collectionId) return;
    set({ error: undefined });
    try {
      await api.deleteRequest(requestId);
      const current = get();
      const tabs = current.tabs.filter((tab) => tab.requestId !== requestId);
      const activeDeleted = current.activeRequestId === requestId;
      useDraftStore.getState().removeMany([requestId]);
      useResponseStore.getState().removeMany([requestId]);
      set({
        tree: removeRequestNode(current.tree, requestId),
        tabs,
        activeRequestId: activeDeleted ? undefined : current.activeRequestId,
        requestLoading: activeDeleted ? false : current.requestLoading,
      });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  deleteNode: async (node) => {
    const collectionId = get().activeCollectionId;
    if (!collectionId) return;
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
        requestLoading: activeDeleted ? false : current.requestLoading,
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
      set((state) => ({
        tree: moveNodeInTree(state.tree, node, parentId ?? null, position),
      }));
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

    useDraftStore.getState().setDraft(request, true);
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
      useDraftStore.getState().markSaved(request.id);
      set((state) => ({
        saving: false,
        tabs: upsertTab(state.tabs, request, false, true),
        tree: renameRequestNode(state.tree, request.id, request.name),
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
        getWorkspaceUiState().activeEnvironmentId ?? null,
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
    if (!request) return;
    set({ sending: true, error: undefined });
    try {
      const response = await api.sendRequest(
        request,
        getWorkspaceUiState().activeEnvironmentId ?? null,
      );
      useResponseStore.getState().setResponse(request.id, response);
      if (get().activeRequestId === request.id) {
        set({ sending: false });
      } else {
        set({ sending: false });
      }
      await get().resolveActiveRequest();
    } catch (error) {
      set({
        error:
          get().activeRequestId === request.id ? String(error) : get().error,
        sending: false,
      });
      await get().resolveActiveRequest();
    }
  },

  toggleSidebar: () =>
    set((state) => ({ sidebarVisible: !state.sidebarVisible })),
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
      return {
        ...node,
        children: renameRequestNode(node.children, requestId, name),
      };
    }
    return node;
  });
}

function insertNodeAt(
  nodes: CollectionNode[],
  parentId: string | null,
  position: number,
  node: CollectionNode,
): CollectionNode[] {
  if (parentId === null) {
    return insertIntoSiblings(nodes, position, { ...node, parentId: null });
  }

  let changed = false;
  const next = nodes.map((item) => {
    if (item.id === parentId) {
      changed = true;
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
  return changed || next.some((item, index) => item !== nodes[index])
    ? next
    : nodes;
}

function insertIntoSiblings(
  siblings: CollectionNode[],
  position: number,
  node: CollectionNode,
): CollectionNode[] {
  const index = Math.max(0, Math.min(position, siblings.length));
  const next = [...siblings.slice(0, index), node, ...siblings.slice(index)];
  return reindexSiblings(next);
}

function removeNodeById(
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
    if (node.children.length) {
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

function removeRequestNode(
  nodes: CollectionNode[],
  requestId: string,
): CollectionNode[] {
  let changed = false;
  const next: CollectionNode[] = [];
  for (const node of nodes) {
    if (node.requestId === requestId) {
      changed = true;
      continue;
    }
    if (node.children.length) {
      const children = removeRequestNode(node.children, requestId);
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

function moveNodeInTree(
  nodes: CollectionNode[],
  node: CollectionNode,
  parentId: string | null,
  position: number,
): CollectionNode[] {
  const withoutNode = removeNodeById(nodes, node.id);
  return insertNodeAt(withoutNode, parentId, position, {
    ...node,
    parentId,
    position,
  });
}

function findRequestNodeByRequestId(
  nodes: CollectionNode[],
  requestId: string,
): CollectionNode | undefined {
  for (const node of nodes) {
    if (node.requestId === requestId) return node;
    const child = findRequestNodeByRequestId(node.children, requestId);
    if (child) return child;
  }
  return undefined;
}

function reindexSiblings(nodes: CollectionNode[]): CollectionNode[] {
  return nodes.map((node, position) =>
    node.position === position ? node : { ...node, position },
  );
}

function requestIdsForNode(node: CollectionNode): string[] {
  return [
    ...(node.requestId ? [node.requestId] : []),
    ...node.children.flatMap(requestIdsForNode),
  ];
}

function normalizeRequestParams(request: RequestDetail): RequestDetail {
  return {
    ...request,
    query: deriveQueryRows(request.url, request.query ?? []),
    pathParams: derivePathParamRows(request.url, request.pathParams ?? []),
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
  const exists = tabs.some((item) => item.requestId === request.id);
  if (!exists) return [...tabs, tab];
  return tabs.map((item) =>
    item.requestId === request.id
      ? { ...item, ...tab, dirty: replaceDirty ? dirty : item.dirty }
      : item,
  );
}
