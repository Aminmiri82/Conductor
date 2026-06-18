import { invoke } from "@tauri-apps/api/core";
import type {
  CollectionNode,
  CollectionSummary,
  CreateRequestResult,
  DuplicateRequestResult,
  EnvironmentSummary,
  RequestDetail,
  ResolvedRequestPreview,
  WorkspaceUiState,
  SendRequestResult,
  VariableEntry,
} from "@/features/types";

export const api = {
  listCollections: () => invoke<CollectionSummary[]>("list_collections"),
  listEnvironments: () => invoke<EnvironmentSummary[]>("list_environments"),
  createEnvironment: (name: string) =>
    invoke<string>("create_environment", { input: { name } }),
  renameEnvironment: (environmentId: string, name: string) =>
    invoke<void>("rename_environment", { input: { environmentId, name } }),
  deleteEnvironment: (environmentId: string) =>
    invoke<void>("delete_environment", { environmentId }),
  importEnvironment: (contents: string, fileName?: string | null) =>
    invoke<string>("import_postman_environment", {
      postmanJson: contents,
      fileName,
    }),
  getWorkspaceState: (key: string) =>
    invoke<WorkspaceUiState | null>("get_workspace_state", { key }),
  setWorkspaceState: (key: string, value: WorkspaceUiState) =>
    invoke<void>("set_workspace_state", { key, value }),
  getCollectionTree: (collectionId: string) =>
    invoke<CollectionNode[]>("get_collection_tree", { collectionId }),
  importPostmanCollection: (postmanJson: string) =>
    invoke<string>("import_postman_collection", { postmanJson }),
  getRequest: (requestId: string) =>
    invoke<RequestDetail>("get_request", { requestId }),
  createRequest: (
    collectionId: string,
    parentId: string | null | undefined,
    position: number,
    name: string,
  ) =>
    invoke<CreateRequestResult>("create_request", {
      input: { collectionId, parentId, position, name },
    }),
  createFolder: (
    collectionId: string,
    parentId: string | null | undefined,
    position: number,
    name: string,
  ) =>
    invoke<string>("create_folder", {
      input: { collectionId, parentId, position, name },
    }),
  duplicateRequest: (requestId: string) =>
    invoke<DuplicateRequestResult>("duplicate_request", { input: { requestId } }),
  deleteRequest: (requestId: string) =>
    invoke<void>("delete_request", { requestId }),
  deleteNode: (nodeId: string) =>
    invoke<void>("delete_node", { nodeId }),
  moveNode: (
    nodeId: string,
    parentId: string | null | undefined,
    position: number,
  ) => invoke<void>("move_node", { input: { nodeId, parentId, position } }),
  saveTextFile: (path: string, contents: string) =>
    invoke<void>("save_text_file", { input: { path, contents } }),
  saveRequest: (request: RequestDetail) =>
    invoke<void>("save_request", { request }),
  resolveRequest: (request: RequestDetail, environmentId?: string | null) =>
    invoke<ResolvedRequestPreview>("resolve_request", { request, environmentId }),
  sendRequest: (request: RequestDetail, environmentId?: string | null) =>
    invoke<SendRequestResult>("send_request", { input: { request, environmentId } }),
  listVariables: (
    scope: "global" | "collection" | "environment",
    collectionId?: string | null,
    environmentId?: string | null,
  ) => invoke<VariableEntry[]>("list_variables", { scope, collectionId, environmentId }),
  saveVariables: (
    scope: "global" | "collection" | "environment",
    collectionId: string | null | undefined,
    environmentId: string | null | undefined,
    variables: VariableEntry[],
  ) => invoke<void>("save_variables", { scope, collectionId, environmentId, variables }),
};
