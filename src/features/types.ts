// IPC types are generated from Rust into `src/bindings.ts` (`pnpm bindings`).
// Only frontend-owned types are defined here.
export type {
  AuthConfig,
  BodyField,
  CollectionNode,
  CollectionSummary,
  CreateRequestResult,
  DuplicateRequestResult,
  EnvironmentSummary,
  KeyValue,
  RequestBody,
  RequestDetail,
  ResolvedRequestPreview,
  ResponseHeader,
  SendRequestResult,
  UnresolvedVariable,
  VariableEntry,
} from "@/bindings";

export type VariableScope = "global" | "collection" | "environment";

export type RequestEditorTab = "params" | "headers" | "auth" | "body" | "variables";

export type AppTheme = "softpro" | "conductor" | "brutalist";

export type UrlDisplayMode = "flat" | "syntax" | "chip" | "hybrid";

export type SettingsTab = "appearance" | "variables" | "shortcuts" | "about";

export type WorkspaceUiState = {
  activeCollectionId?: string;
  activeEnvironmentId?: string | null;
  appTheme: AppTheme;
  accentColor: string;
  urlDisplayMode: UrlDisplayMode;
  settingsTab: SettingsTab;
  requestEditorTabs: Record<string, RequestEditorTab>;
};
