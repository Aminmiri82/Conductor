import { commands, type RequestDetail, type VariableEntry } from "@/bindings";
import type { VariableScope, WorkspaceUiState } from "@/features/types";

// Call-site conveniences over the generated `commands`. Command names, argument
// names, and payload types come from Rust; add new commands in Rust and run
// `pnpm bindings` rather than calling `invoke` directly.
export const api = {
  listCollections: commands.listCollections,
  listEnvironments: commands.listEnvironments,
  createEnvironment: (name: string) => commands.createEnvironment({ name }),
  renameEnvironment: (environmentId: string, name: string) =>
    commands.renameEnvironment({ environmentId, name }),
  deleteEnvironment: commands.deleteEnvironment,
  importEnvironment: (contents: string, fileName?: string | null) =>
    commands.importPostmanEnvironment(contents, fileName ?? null),
  // Workspace UI state is opaque JSON to Rust; the store normalizes it on load.
  getWorkspaceState: (key: string) =>
    commands.getWorkspaceState(key) as Promise<WorkspaceUiState | null>,
  setWorkspaceState: (key: string, value: WorkspaceUiState) =>
    commands.setWorkspaceState(key, value),
  getCollectionTree: commands.getCollectionTree,
  importPostmanCollection: commands.importPostmanCollection,
  getRequest: commands.getRequest,
  createRequest: (
    collectionId: string,
    parentId: string | null | undefined,
    position: number,
    name: string,
  ) =>
    commands.createRequest({
      collectionId,
      parentId: parentId ?? null,
      position,
      name,
    }),
  createFolder: (
    collectionId: string,
    parentId: string | null | undefined,
    position: number,
    name: string,
  ) =>
    commands.createFolder({
      collectionId,
      parentId: parentId ?? null,
      position,
      name,
    }),
  duplicateRequest: (requestId: string) =>
    commands.duplicateRequest({ requestId }),
  deleteRequest: commands.deleteRequest,
  deleteNode: commands.deleteNode,
  moveNode: (
    nodeId: string,
    parentId: string | null | undefined,
    position: number,
  ) => commands.moveNode({ nodeId, parentId: parentId ?? null, position }),
  saveTextFile: (path: string, contents: string) =>
    commands.saveTextFile({ path, contents }),
  saveResponseBody: (historyId: string, path: string) =>
    commands.saveResponseBody({ historyId, path }),
  saveRequest: commands.saveRequest,
  resolveRequest: (request: RequestDetail, environmentId?: string | null) =>
    commands.resolveRequest(request, environmentId ?? null),
  sendRequest: (request: RequestDetail, environmentId?: string | null) =>
    commands.sendRequest({ request, environmentId }),
  listVariables: (
    scope: VariableScope,
    collectionId?: string | null,
    environmentId?: string | null,
  ) =>
    commands.listVariables(scope, collectionId ?? null, environmentId ?? null),
  saveVariables: (
    scope: VariableScope,
    collectionId: string | null | undefined,
    environmentId: string | null | undefined,
    variables: VariableEntry[],
  ) =>
    commands.saveVariables(
      scope,
      collectionId ?? null,
      environmentId ?? null,
      variables,
    ),
};
