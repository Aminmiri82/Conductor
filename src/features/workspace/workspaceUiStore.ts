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
    environments: Promise<EnvironmentSummary[]>,
  ) => Promise<WorkspaceUiState>;
  reconcileActiveEnvironments: (environments: EnvironmentSummary[]) => void;
  setActiveCollectionId: (collectionId: string) => void;
  setActiveEnvironmentId: (
    collectionId: string,
    environmentId: string | null,
  ) => void;
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
  activeEnvironmentIds: {},
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

    // Takes the environments as a promise so startup can fetch them and the
    // stored state at the same time.
    loadWorkspaceUiState: async (environmentsPromise) => {
      const [stored, environments] = await Promise.all([
        api.getWorkspaceState(WORKSPACE_UI_STATE_KEY),
        environmentsPromise,
      ]);
      const workspaceUi = normalizeWorkspaceUiState(stored);
      const nextWorkspaceUi = {
        ...workspaceUi,
        activeEnvironmentIds: validEnvironmentIds(
          environments,
          workspaceUi.activeEnvironmentIds,
        ),
      };
      set({ workspaceUi: nextWorkspaceUi, workspaceUiDirty: false });
      return nextWorkspaceUi;
    },

    reconcileActiveEnvironments: (environments) => {
      const previous = get().workspaceUi.activeEnvironmentIds;
      const activeEnvironmentIds = validEnvironmentIds(environments, previous);
      if (
        Object.keys(activeEnvironmentIds).length !==
        Object.keys(previous).length
      ) {
        set((state) => ({
          workspaceUi: { ...state.workspaceUi, activeEnvironmentIds },
          workspaceUiDirty: true,
        }));
        get().scheduleWorkspaceUiStateFlush();
      }
    },

    setActiveCollectionId: (collectionId) => {
      set((state) => ({
        workspaceUi: { ...state.workspaceUi, activeCollectionId: collectionId },
        workspaceUiDirty: true,
      }));
    },

    setActiveEnvironmentId: (collectionId, environmentId) => {
      set((state) => {
        const { [collectionId]: _previous, ...others } =
          state.workspaceUi.activeEnvironmentIds;
        return {
          workspaceUi: {
            ...state.workspaceUi,
            activeEnvironmentIds: environmentId
              ? { ...others, [collectionId]: environmentId }
              : others,
          },
          workspaceUiDirty: true,
        };
      });
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
    activeEnvironmentIds:
      value.activeEnvironmentIds &&
      typeof value.activeEnvironmentIds === "object"
        ? Object.fromEntries(
            Object.entries(value.activeEnvironmentIds).filter(
              (entry) => typeof entry[1] === "string",
            ),
          )
        : {},
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

/** Drops choices whose environment is gone or not owned by that collection. */
function validEnvironmentIds(
  environments: EnvironmentSummary[],
  activeEnvironmentIds: Record<string, string>,
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(activeEnvironmentIds).filter(([collectionId, id]) =>
      environments.some(
        (environment) =>
          environment.id === id && environment.collectionId === collectionId,
      ),
    ),
  );
}

/** The active environment of a collection, or null for none. */
export function activeEnvironmentIdFor(
  workspaceUi: WorkspaceUiState,
  collectionId: string | undefined,
): string | null {
  return collectionId
    ? (workspaceUi.activeEnvironmentIds[collectionId] ?? null)
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
