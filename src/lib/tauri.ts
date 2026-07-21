import { invoke } from "@tauri-apps/api/core";
import type {
  CollectionNode,
  CollectionSummary,
  CreateRequestResult,
  DatabaseStatus,
  DuplicateRequestResult,
  EntityId,
  EnvironmentSummary,
  LearnedRowidBenchmark,
  RequestDetail,
  ResolvedRequestPreview,
  WorkspaceUiState,
  SendRequestResult,
  VariableEntry,
} from "@/features/types";

export const api = {
  databaseStatus: () => invoke<DatabaseStatus>("database_status"),
  setLearnedRowidEnabled: (enabled: boolean) =>
    invoke<DatabaseStatus>("set_learned_rowid_enabled", { enabled }),
  resetLearnedRowidCounters: () =>
    invoke<DatabaseStatus>("reset_learned_rowid_counters"),
  rebuildLearnedRowidModels: () =>
    invoke<DatabaseStatus>("rebuild_learned_rowid_models"),
  runLearnedRowidBenchmark: () =>
    invoke<LearnedRowidBenchmark>("run_learned_rowid_benchmark"),
  listCollections: () => invoke<CollectionSummary[]>("list_collections"),
  listEnvironments: () => invoke<EnvironmentSummary[]>("list_environments"),
  createEnvironment: (name: string) =>
    invoke<EntityId>("create_environment", { input: { name } }),
  renameEnvironment: (environmentId: EntityId, name: string) =>
    invoke<void>("rename_environment", { input: { environmentId, name } }),
  deleteEnvironment: (environmentId: EntityId) =>
    invoke<void>("delete_environment", { environmentId }),
  importEnvironment: (contents: string, fileName?: string | null) =>
    invoke<EntityId>("import_postman_environment", {
      postmanJson: contents,
      fileName,
    }),
  getWorkspaceState: (key: string) =>
    invoke<WorkspaceUiState | null>("get_workspace_state", { key }),
  setWorkspaceState: (key: string, value: WorkspaceUiState) =>
    invoke<void>("set_workspace_state", { key, value }),
  getCollectionTree: (collectionId: EntityId) =>
    invoke<CollectionNode[]>("get_collection_tree", { collectionId }),
  importPostmanCollection: (postmanJson: string) =>
    invoke<EntityId>("import_postman_collection", { postmanJson }),
  getRequest: (requestId: EntityId) =>
    invoke<RequestDetail>("get_request", { requestId }),
  createRequest: (
    collectionId: EntityId,
    parentId: EntityId | null | undefined,
    position: number,
    name: string,
  ) =>
    invoke<CreateRequestResult>("create_request", {
      input: { collectionId, parentId, position, name },
    }),
  createFolder: (
    collectionId: EntityId,
    parentId: EntityId | null | undefined,
    position: number,
    name: string,
  ) =>
    invoke<EntityId>("create_folder", {
      input: { collectionId, parentId, position, name },
    }),
  duplicateRequest: (requestId: EntityId) =>
    invoke<DuplicateRequestResult>("duplicate_request", { input: { requestId } }),
  deleteRequest: (requestId: EntityId) =>
    invoke<void>("delete_request", { requestId }),
  deleteNode: (nodeId: EntityId) =>
    invoke<void>("delete_node", { nodeId }),
  moveNode: (
    nodeId: EntityId,
    parentId: EntityId | null | undefined,
    position: number,
  ) => invoke<void>("move_node", { input: { nodeId, parentId, position } }),
  saveTextFile: (path: string, contents: string) =>
    invoke<void>("save_text_file", { input: { path, contents } }),
  saveRequest: (request: RequestDetail) =>
    invoke<void>("save_request", { request }),
  resolveRequest: (request: RequestDetail, environmentId?: EntityId | null) =>
    invoke<ResolvedRequestPreview>("resolve_request", { request, environmentId }),
  sendRequest: (request: RequestDetail, environmentId?: EntityId | null) =>
    invoke<SendRequestResult>("send_request", { input: { request, environmentId } }),
  listVariables: (
    scope: "global" | "collection" | "environment",
    collectionId?: EntityId | null,
    environmentId?: EntityId | null,
  ) => invoke<VariableEntry[]>("list_variables", { scope, collectionId, environmentId }),
  saveVariables: (
    scope: "global" | "collection" | "environment",
    collectionId: EntityId | null | undefined,
    environmentId: EntityId | null | undefined,
    variables: VariableEntry[],
  ) => invoke<void>("save_variables", { scope, collectionId, environmentId, variables }),
};
