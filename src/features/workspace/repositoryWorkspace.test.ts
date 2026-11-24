import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import type { RepositoryRefresh, RequestDetail } from "@/features/types";
import { getDraft, useDraftStore } from "./draftStore";
import {
  applyRepositoryRefresh,
  resolveRepositoryConflict,
} from "./repositoryWorkspace";
import { useWorkspaceStore } from "./workspaceStore";

const request: RequestDetail = {
  id: "request",
  collectionId: "repository",
  repositoryRevision: "original",
  name: "Draft",
  method: "POST",
  url: "http://127.0.0.1/draft",
  headers: [],
  query: [],
  pathParams: [],
  auth: null,
  body: null,
  preRequestScript: null,
  testScript: null,
};
const tab = {
  requestId: request.id,
  name: request.name,
  method: request.method,
  dirty: true,
};
const removed = {
  requestId: request.id,
  revision: null,
  name: null,
  method: null,
};
const resolved = {
  inheritedAuth: null,
  unresolvedVariables: [],
  variableValues: {},
};
const initial = useWorkspaceStore.getState();

function refresh(
  changedRequests: RepositoryRefresh["changedRequests"],
): RepositoryRefresh {
  return {
    collections: null,
    changedRequests,
    changedCollectionIds: [request.collectionId],
    changedEnvironmentIds: [],
    changedVariableCollectionIds: [],
  };
}

beforeEach(() => {
  vi.stubGlobal("window", {});
  useWorkspaceStore.setState({
    activeCollectionId: request.collectionId,
    activeRequestId: request.id,
    tabs: [tab],
    tree: [],
  });
  useDraftStore.getState().setDraft(request);
});

afterEach(() => {
  clearMocks();
  vi.unstubAllGlobals();
  useDraftStore.getState().clearAll();
  useWorkspaceStore.setState(initial);
});

it("keeps the latest local tree and marks a removed draft if both tree reads race local edits", async () => {
  let attempts = 0;
  let anotherRefresh = false;
  const loadedTree = [
    {
      id: "external",
      kind: "folder",
      name: "External folder",
      parentId: null,
      position: 0,
      children: [],
    },
  ];
  mockIPC((command) => {
    if (command !== "get_collection_tree") {
      throw new Error(`Unexpected IPC command: ${command}`);
    }
    if (anotherRefresh) return loadedTree;
    if (++attempts > 2) throw new Error("Repeated tree reads");
    useWorkspaceStore.setState({ tree: [] });
    return loadedTree;
  });

  await applyRepositoryRefresh(refresh([removed]));

  expect(getDraft(request.id)).toBe(request);
  expect(useWorkspaceStore.getState().tabs).toEqual([
    { ...tab, conflict: { revision: null } },
  ]);
  expect(useWorkspaceStore.getState().tree).toEqual([]);
  expect(useWorkspaceStore.getState().error).toBeUndefined();

  anotherRefresh = true;
  await applyRepositoryRefresh({ ...refresh([]), changedCollectionIds: [] });
  expect(useWorkspaceStore.getState().tree).toEqual(loadedTree);
});

it("clears a removed draft preview and closes the draft when the user reloads", async () => {
  useDraftStore.getState().setPreview(request.id, resolved);
  mockIPC((command) => {
    if (command === "save_request") {
      return { status: "conflict", revision: null, refresh: null };
    }
    throw new Error(`Unexpected IPC command: ${command}`);
  });

  await useWorkspaceStore.getState().saveActiveRequest();
  expect(getDraft(request.id)).toBe(request);
  expect(useDraftStore.getState().previews.has(request.id)).toBe(false);
  expect(useWorkspaceStore.getState().tabs).toEqual([
    { ...tab, conflict: { revision: null } },
  ]);

  await resolveRepositoryConflict("reload");
  expect(getDraft(request.id)).toBeUndefined();
  expect(useWorkspaceStore.getState().activeRequestId).toBeUndefined();
  expect(useWorkspaceStore.getState().tabs).toEqual([]);
});

it("keeps edits made while an overwrite saves and gives them its new revision", async () => {
  mockIPC((command, args) => {
    if (command === "save_request") {
      const revision = (args as { request: RequestDetail }).request
        .repositoryRevision;
      if (revision === "original") {
        return { status: "conflict", revision: "external", refresh: null };
      }
      if (revision === "external") {
        useWorkspaceStore
          .getState()
          .updateRequest({ name: "Edited during save" });
        return { status: "saved", revision: "written", refresh: null };
      }
    }
    if (command === "resolve_request") return resolved;
    throw new Error(`Unexpected IPC command: ${command}`);
  });

  await useWorkspaceStore.getState().saveActiveRequest();
  expect(useWorkspaceStore.getState().tabs).toEqual([
    { ...tab, conflict: { revision: "external" } },
  ]);

  await resolveRepositoryConflict("keep");
  expect(getDraft(request.id)).toMatchObject({
    name: "Edited during save",
    repositoryRevision: "written",
  });
  expect(useWorkspaceStore.getState().tabs).toEqual([
    { ...tab, name: "Edited during save" },
  ]);
  expect(useWorkspaceStore.getState().saving).toBe(false);
});

it("saves a deleted draft as a new request and preserves edits made during recovery", async () => {
  const saved = {
    ...request,
    id: "copy",
    repositoryRevision: "new",
  };
  useWorkspaceStore.setState({
    tabs: [{ ...tab, conflict: { revision: null } }],
  });
  mockIPC((command) => {
    switch (command) {
      case "save_repository_request_as_new":
        useWorkspaceStore
          .getState()
          .updateRequest({ name: "Edited during recovery" });
        return { requestId: saved.id };
      case "get_collection_tree":
        return [];
      case "get_request":
        return saved;
      case "resolve_request":
        return resolved;
      default:
        throw new Error(`Unexpected IPC command: ${command}`);
    }
  });

  await resolveRepositoryConflict("save-as-new");
  expect(getDraft(request.id)).toBeUndefined();
  expect(getDraft(saved.id)).toMatchObject({
    name: "Edited during recovery",
    repositoryRevision: "new",
  });
  expect(useWorkspaceStore.getState().activeRequestId).toBe(saved.id);
  expect(useWorkspaceStore.getState().tabs).toEqual([
    { ...tab, requestId: saved.id, name: "Edited during recovery" },
  ]);
});

it("keeps a draft editable when the repository rejects a save", async () => {
  mockIPC((command) => {
    if (command === "save_request") {
      return {
        status: "failed",
        message: "Repository is read-only",
        refresh: { ...refresh([]), changedCollectionIds: [] },
      };
    }
    throw new Error(`Unexpected IPC command: ${command}`);
  });

  await useWorkspaceStore.getState().saveActiveRequest();
  expect(getDraft(request.id)).toBe(request);
  expect(useWorkspaceStore.getState().tabs).toEqual([tab]);
  expect(useWorkspaceStore.getState().error).toBe("Repository is read-only");
  expect(useWorkspaceStore.getState().saving).toBe(false);
});
