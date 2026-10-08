// IPC types are generated from Rust into `src/bindings.ts` (`pnpm bindings`).
// Only frontend-owned types are defined here.
import type { CollectionNode, VariableTarget } from "@/bindings";

export type {
  ApiKeyLocation,
  AuthConfig,
  AuthType,
  BodyField,
  BodyFieldType,
  BodyMode,
  CollectionNode,
  CollectionSummary,
  EnvironmentSummary,
  KeyValue,
  RequestBody,
  RequestDetail,
  ResolvedRequestPreview,
  SendRequestResult,
  VariableChange,
  VariableEntry,
  VariableTarget,
} from "@/bindings";

export type FolderNode = Extract<CollectionNode, { kind: "folder" }>;
export type RequestNode = Extract<CollectionNode, { kind: "request" }>;

export type VariableScope = VariableTarget["scope"];

export type RequestEditorTab =
  "params" | "headers" | "auth" | "body" | "variables";

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
