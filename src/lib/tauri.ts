import { commands, type RequestDetail } from "@/bindings";
import type { WorkspaceUiState } from "@/features/types";

const utf8 = new TextDecoder();

// Call-site conveniences over the generated `commands`. Command names, argument
// names, and payload types come from Rust; add new commands in Rust and run
// `pnpm bindings` rather than calling `invoke` directly.
export const api = {
  listCollections: commands.listCollections,
  listEnvironments: commands.listEnvironments,
  createEnvironment: (collectionId: string, name: string) =>
    commands.createEnvironment({ collectionId, name }),
  duplicateEnvironment: (environmentId: string) =>
    commands.duplicateEnvironment({ environmentId }),
  renameEnvironment: (environmentId: string, name: string) =>
    commands.renameEnvironment({ environmentId, name }),
  deleteEnvironment: commands.deleteEnvironment,
  importEnvironment: (
    collectionId: string,
    contents: string,
    fileName: string,
  ) => commands.importPostmanEnvironment(collectionId, contents, fileName),
  // Workspace UI state is opaque JSON to Rust; the store normalizes it on load.
  getWorkspaceState: (key: string) =>
    commands.getWorkspaceState(key) as Promise<WorkspaceUiState | null>,
  setWorkspaceState: (key: string, value: WorkspaceUiState) =>
    commands.setWorkspaceState(key, value),
  getCollectionTree: commands.getCollectionTree,
  importPostmanCollection: commands.importPostmanCollection,
  pickAndImportPostmanCollection: commands.pickAndImportPostmanCollection,
  exportPostmanCollection: commands.exportPostmanCollection,
  createCollection: commands.createCollection,
  deleteCollection: commands.deleteCollection,
  renameCollection: commands.renameCollection,
  getCollectionProfile: commands.getCollectionProfile,
  getRequest: commands.getRequest,
  createRequest: (
    collectionId: string,
    parentId: string | null,
    position: number,
    name: string,
  ) => commands.createRequest({ collectionId, parentId, position, name }),
  createFolder: (
    collectionId: string,
    parentId: string | null,
    position: number,
    name: string,
  ) => commands.createFolder({ collectionId, parentId, position, name }),
  duplicateRequest: (requestId: string) =>
    commands.duplicateRequest({ requestId }),
  deleteNode: commands.deleteNode,
  renameFolder: commands.renameFolder,
  listCollectionStats: commands.listCollectionStats,
  moveNode: (nodeId: string, parentId: string | null, position: number) =>
    commands.moveNode({ nodeId, parentId, position }),
  // Arrives as raw UTF-8 rather than a JSON string, so a large body is not
  // parsed out of an escaped copy of itself.
  getResponseBody: async (historyId: string) =>
    utf8.decode(await commands.getResponseBody({ historyId })),
  discardResponses: (requestIds: string[]) =>
    commands.discardResponses({ requestIds }),
  saveResponseBody: (historyId: string, path: string) =>
    commands.saveResponseBody({ historyId, path }),
  saveRequest: commands.saveRequest,
  resolveRequest: (request: RequestDetail, environmentId: string | null) =>
    commands.resolveRequest(request, environmentId),
  sendRequest: (request: RequestDetail, environmentId: string | null) =>
    commands.sendRequest({ request, environmentId }),
  listVariables: commands.listVariables,
  applyVariableChanges: commands.applyVariableChanges,
};
