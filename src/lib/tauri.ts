import { invoke } from "@tauri-apps/api/core";
import type {
  CollectionNode,
  CollectionSummary,
  RequestDetail,
  ResolvedRequestPreview,
  SendRequestResult,
  VariableEntry,
} from "@/features/types";

export const api = {
  listCollections: () => invoke<CollectionSummary[]>("list_collections"),
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
    invoke<string>("create_request", {
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
    invoke<string>("duplicate_request", { input: { requestId } }),
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
  resolveRequest: (request: RequestDetail) =>
    invoke<ResolvedRequestPreview>("resolve_request", { request }),
  sendRequest: (request: RequestDetail) =>
    invoke<SendRequestResult>("send_request", { input: { request } }),
  listVariables: (scopeKind: string, scopeId: string) =>
    invoke<VariableEntry[]>("list_variables", { scopeKind, scopeId }),
  saveVariables: (
    scopeKind: string,
    scopeId: string,
    variables: VariableEntry[],
  ) => invoke<void>("save_variables", { scopeKind, scopeId, variables }),
};
