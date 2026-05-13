import { create } from "zustand";
import { api } from "@/lib/tauri";
import {
  applyPathParamRowChanges,
  buildQueryString,
  derivePathParamRows,
  deriveQueryRows,
  replaceQueryInUrl,
} from "@/features/requests/urlParams";
import {
  getDraft,
  useDraftStore,
} from "@/features/workspace/draftStore";
import { useResponseStore } from "@/features/workspace/responseStore";
import type {
  AppTheme,
  CollectionNode,
  CollectionSummary,
  RequestEditorTab,
  RequestDetail,
  SettingsTab,
  UrlDisplayMode,
  WorkspaceUiState,
} from "@/features/types";

export type RequestTab = {
  requestId: string;
  name: string;
  method: string;
  dirty: boolean;
};

type WorkspaceState = {
  collections: CollectionSummary[];
  tree: CollectionNode[];
  tabs: RequestTab[];
  activeCollectionId?: string;
  activeRequestId?: string;
  workspaceUi: WorkspaceUiState;
  workspaceUiDirty: boolean;
  sidebarVisible: boolean;
  collectionLoading: boolean;
  requestLoading: boolean;
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
  setRequestEditorTab: (requestId: string, tab: RequestEditorTab) => void;
  setWorkspacePreference: <K extends keyof WorkspaceUiState>(
    key: K,
    value: WorkspaceUiState[K],
  ) => void;
  scheduleWorkspaceUiStateFlush: () => void;
  flushWorkspaceUiState: () => Promise<void>;
  toggleSidebar: () => void;
  clearError: () => void;
};

const WORKSPACE_UI_STATE_KEY = "workspace.ui";
const WORKSPACE_UI_STATE_FLUSH_DELAY_MS = 300;
const DEFAULT_WORKSPACE_UI_STATE: WorkspaceUiState = {
  appTheme: "softpro",
  accentColor: "#a78bfa",
  urlDisplayMode: "chip",
  settingsTab: "appearance",
  requestEditorTabs: {},
};
let workspaceUiStateFlushTimer: ReturnType<typeof window.setTimeout> | undefined;
let workspaceUiStateFlushPromise = Promise.resolve();

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  collections: [],
  tree: [],
  tabs: [],
  workspaceUi: DEFAULT_WORKSPACE_UI_STATE,
  workspaceUiDirty: false,
  sidebarVisible: true,
  collectionLoading: false,
  requestLoading: false,
  saving: false,
  sending: false,

  loadCollections: async () => {
    set({ collectionLoading: true, error: undefined });
    try {
      const collections = await api.listCollections();
      const workspaceUi = normalizeWorkspaceUiState(
        await api.getWorkspaceState(WORKSPACE_UI_STATE_KEY),
      );
      set({
        collections,
        workspaceUi,
        workspaceUiDirty: false,
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
        workspaceUi: withActiveCollection(get().workspaceUi, collectionId),
        workspaceUiDirty: true,
        collectionLoading: false,
        requestLoading: false,
      });
      useDraftStore.getState().clearAll();
      useResponseStore.getState().clearAll();
      await get().flushWorkspaceUiState();
    } catch (error) {
      set({ error: String(error), collectionLoading: false });
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
        workspaceUi: withActiveCollection(get().workspaceUi, collectionId),
        workspaceUiDirty: true,
        collectionLoading: false,
        requestLoading: false,
      });
      useDraftStore.getState().clearAll();
      useResponseStore.getState().clearAll();
      await get().flushWorkspaceUiState();
    } catch (error) {
      set({ error: String(error), collectionLoading: false });
    }
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
    const tabIndex = current.tabs.findIndex((tab) => tab.requestId === requestId);
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
    set({ error: undefined });
    try {
      await api.deleteRequest(requestId);
      const tree = await api.getCollectionTree(collectionId);
      const current = get();
      const tabs = current.tabs.filter((tab) => tab.requestId !== requestId);
      const activeDeleted = current.activeRequestId === requestId;
      useDraftStore.getState().removeMany([requestId]);
      useResponseStore.getState().removeMany([requestId]);
      set({
        tree,
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
      const tree = await api.getCollectionTree(collectionId);
      const removedRequestIds = requestIdsForNode(node);
      const current = get();
      const activeDeleted =
        current.activeRequestId !== undefined &&
        removedRequestIds.includes(current.activeRequestId);
      useDraftStore.getState().removeMany(removedRequestIds);
      useResponseStore.getState().removeMany(removedRequestIds);
      set({
        tree,
        tabs: current.tabs.filter((tab) => !removedRequestIds.includes(tab.requestId)),
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
      const tree = await api.getCollectionTree(collectionId);
      set({ tree });
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
      const nextUrl = replaceQueryInUrl(current.url, buildQueryString(patch.query));
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
        error: get().activeRequestId === request.id ? String(error) : get().error,
        saving: false,
      });
    }
  },

  resolveActiveRequest: async () => {
    const request = getDraft(get().activeRequestId);
    if (!request) return;
    try {
      const resolvedPreview = await api.resolveRequest(request);
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
      const response = await api.sendRequest(request);
      useResponseStore.getState().setResponse(request.id, response);
      if (get().activeRequestId === request.id) {
        set({ sending: false });
      } else {
        set({ sending: false });
      }
      await get().resolveActiveRequest();
    } catch (error) {
      set({
        error: get().activeRequestId === request.id ? String(error) : get().error,
        sending: false,
      });
      await get().resolveActiveRequest();
    }
  },

  setRequestEditorTab: (requestId, tab) => {
    set((state) => ({
      workspaceUi: {
        ...state.workspaceUi,
        requestEditorTabs: {
          ...state.workspaceUi.requestEditorTabs,
          [requestId]: tab,
        },
      },
      workspaceUiDirty: true,
    }));
    get().scheduleWorkspaceUiStateFlush();
  },

  setWorkspacePreference: (key, value) => {
    set((state) => ({
      workspaceUi: {
        ...state.workspaceUi,
        [key]: value,
      },
      workspaceUiDirty: true,
    }));
    get().scheduleWorkspaceUiStateFlush();
  },

  scheduleWorkspaceUiStateFlush: () => {
    if (workspaceUiStateFlushTimer) {
      window.clearTimeout(workspaceUiStateFlushTimer);
    }
    workspaceUiStateFlushTimer = window.setTimeout(() => {
      workspaceUiStateFlushTimer = undefined;
      void get().flushWorkspaceUiState();
    }, WORKSPACE_UI_STATE_FLUSH_DELAY_MS);
  },

  flushWorkspaceUiState: async () => {
    if (workspaceUiStateFlushTimer) {
      window.clearTimeout(workspaceUiStateFlushTimer);
      workspaceUiStateFlushTimer = undefined;
    }

    const runFlush = async () => {
      const { workspaceUi, workspaceUiDirty } = get();
      if (!workspaceUiDirty) return;
      try {
        await api.setWorkspaceState(WORKSPACE_UI_STATE_KEY, workspaceUi);
        if (get().workspaceUi === workspaceUi) {
          set({ workspaceUiDirty: false });
        }
      } catch {
        // Workspace UI state is a best-effort preference cache.
      }
    };

    workspaceUiStateFlushPromise = workspaceUiStateFlushPromise.then(
      runFlush,
      runFlush,
    );
    await workspaceUiStateFlushPromise;
  },

  toggleSidebar: () => set((state) => ({ sidebarVisible: !state.sidebarVisible })),
  clearError: () => set({ error: undefined }),
}));

function normalizeWorkspaceUiState(
  value: WorkspaceUiState | null | undefined,
): WorkspaceUiState {
  if (!value || typeof value !== "object") return DEFAULT_WORKSPACE_UI_STATE;
  return {
    ...DEFAULT_WORKSPACE_UI_STATE,
    activeCollectionId:
      typeof value.activeCollectionId === "string"
        ? value.activeCollectionId
        : undefined,
    appTheme: isAppTheme(value.appTheme)
      ? value.appTheme
      : DEFAULT_WORKSPACE_UI_STATE.appTheme,
    accentColor:
      typeof value.accentColor === "string" && value.accentColor.startsWith("#")
        ? value.accentColor
        : DEFAULT_WORKSPACE_UI_STATE.accentColor,
    urlDisplayMode: isUrlDisplayMode(value.urlDisplayMode)
      ? value.urlDisplayMode
      : DEFAULT_WORKSPACE_UI_STATE.urlDisplayMode,
    settingsTab: isSettingsTab(value.settingsTab)
      ? value.settingsTab
      : DEFAULT_WORKSPACE_UI_STATE.settingsTab,
    requestEditorTabs:
      value.requestEditorTabs && typeof value.requestEditorTabs === "object"
        ? normalizeRequestEditorTabs(value.requestEditorTabs)
        : {},
  };
}

function normalizeRequestEditorTabs(
  tabs: Record<string, RequestEditorTab>,
): Record<string, RequestEditorTab> {
  return Object.fromEntries(
    Object.entries(tabs).filter((entry): entry is [string, RequestEditorTab] =>
      isRequestEditorTab(entry[1]),
    ),
  );
}

function isAppTheme(value: unknown): value is AppTheme {
  return value === "softpro" || value === "conductor" || value === "brutalist";
}

function isUrlDisplayMode(value: unknown): value is UrlDisplayMode {
  return value === "flat" || value === "syntax" || value === "chip" || value === "hybrid";
}

function isSettingsTab(value: unknown): value is SettingsTab {
  return (
    value === "appearance" ||
    value === "variables" ||
    value === "shortcuts" ||
    value === "about"
  );
}

function isRequestEditorTab(value: unknown): value is RequestEditorTab {
  return (
    value === "params" ||
    value === "headers" ||
    value === "auth" ||
    value === "body" ||
    value === "variables"
  );
}

function withActiveCollection(
  workspaceUi: WorkspaceUiState,
  collectionId: string,
): WorkspaceUiState {
  return {
    ...workspaceUi,
    activeCollectionId: collectionId,
  };
}

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
