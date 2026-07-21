import { create } from "zustand";
import { api } from "@/lib/tauri";
import type {
  AppTheme,
  EnvironmentSummary,
  RequestEditorTab,
  SettingsTab,
  UrlDisplayMode,
  WorkspaceUiState,
} from "@/features/types";

type WorkspaceUiStoreState = {
  workspaceUi: WorkspaceUiState;
  workspaceUiDirty: boolean;
  loadWorkspaceUiState: (
    environments: EnvironmentSummary[],
  ) => Promise<WorkspaceUiState>;
  reconcileActiveEnvironment: (
    environments: EnvironmentSummary[],
  ) => string | null;
  setActiveCollectionId: (collectionId: string) => void;
  setActiveEnvironmentId: (environmentId: string | null) => void;
  setRequestEditorTab: (requestId: string, tab: RequestEditorTab) => void;
  setWorkspacePreference: <K extends keyof WorkspaceUiState>(
    key: K,
    value: WorkspaceUiState[K],
  ) => void;
  scheduleWorkspaceUiStateFlush: () => void;
  flushWorkspaceUiState: () => Promise<void>;
};

const WORKSPACE_UI_STATE_KEY = "workspace.ui";
const WORKSPACE_UI_STATE_FLUSH_DELAY_MS = 300;
const DEFAULT_WORKSPACE_UI_STATE: WorkspaceUiState = {
  activeEnvironmentId: null,
  appTheme: "softpro",
  accentColor: "#a78bfa",
  urlDisplayMode: "chip",
  settingsTab: "appearance",
  requestEditorTabs: {},
};
let workspaceUiStateFlushTimer:
  ReturnType<typeof window.setTimeout> | undefined;
let workspaceUiStateFlushPromise = Promise.resolve();

export const useWorkspaceUiStore = create<WorkspaceUiStoreState>(
  (set, get) => ({
    workspaceUi: DEFAULT_WORKSPACE_UI_STATE,
    workspaceUiDirty: false,

    loadWorkspaceUiState: async (environments) => {
      const workspaceUi = normalizeWorkspaceUiState(
        await api.getWorkspaceState(WORKSPACE_UI_STATE_KEY),
      );
      const activeEnvironmentId = validEnvironmentId(
        environments,
        workspaceUi.activeEnvironmentId,
      );
      const nextWorkspaceUi = { ...workspaceUi, activeEnvironmentId };
      set({ workspaceUi: nextWorkspaceUi, workspaceUiDirty: false });
      return nextWorkspaceUi;
    },

    reconcileActiveEnvironment: (environments) => {
      const previousActiveEnvironmentId = get().workspaceUi.activeEnvironmentId;
      const activeEnvironmentId = validEnvironmentId(
        environments,
        previousActiveEnvironmentId,
      );
      if (activeEnvironmentId !== previousActiveEnvironmentId) {
        set((state) => ({
          workspaceUi: { ...state.workspaceUi, activeEnvironmentId },
          workspaceUiDirty: true,
        }));
        get().scheduleWorkspaceUiStateFlush();
      }
      return activeEnvironmentId;
    },

    setActiveCollectionId: (collectionId) => {
      set((state) => ({
        workspaceUi: { ...state.workspaceUi, activeCollectionId: collectionId },
        workspaceUiDirty: true,
      }));
    },

    setActiveEnvironmentId: (environmentId) => {
      set((state) => ({
        workspaceUi: {
          ...state.workspaceUi,
          activeEnvironmentId: environmentId,
        },
        workspaceUiDirty: true,
      }));
      get().scheduleWorkspaceUiStateFlush();
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
  }),
);

export function getWorkspaceUiState(): WorkspaceUiState {
  return useWorkspaceUiStore.getState().workspaceUi;
}

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
    activeEnvironmentId:
      typeof value.activeEnvironmentId === "string"
        ? value.activeEnvironmentId
        : null,
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

function validEnvironmentId(
  environments: EnvironmentSummary[],
  environmentId: string | null | undefined,
): string | null {
  return environments.some((environment) => environment.id === environmentId)
    ? (environmentId ?? null)
    : null;
}

function isAppTheme(value: unknown): value is AppTheme {
  return value === "softpro" || value === "conductor" || value === "brutalist";
}

function isUrlDisplayMode(value: unknown): value is UrlDisplayMode {
  return (
    value === "flat" ||
    value === "syntax" ||
    value === "chip" ||
    value === "hybrid"
  );
}

function isSettingsTab(value: unknown): value is SettingsTab {
  return (
    value === "appearance" ||
    value === "variables" ||
    value === "shortcuts" ||
    value === "data" ||
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
