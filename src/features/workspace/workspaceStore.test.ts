import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { expect, it, vi } from "vite-plus/test";
import { useWorkspaceStore } from "./workspaceStore";

it("preserves an unsaved local draft when the user cancels opening a repository", async () => {
  const before = useWorkspaceStore.getState();
  const tabs = [
    { requestId: "draft", name: "Draft", method: "GET", dirty: true },
  ];
  useWorkspaceStore.setState({ activeCollectionId: "local", tabs });
  vi.stubGlobal("window", {});
  mockIPC((command) => {
    if (command === "plugin:dialog|message") return "Cancel";
    throw new Error(`Unexpected IPC command: ${command}`);
  });

  try {
    await expect(
      useWorkspaceStore.getState().openRepository("/repo", false, null),
    ).resolves.toBe(false);
    expect(useWorkspaceStore.getState().tabs).toEqual(tabs);
    expect(useWorkspaceStore.getState().activeCollectionId).toBe("local");
  } finally {
    clearMocks();
    vi.unstubAllGlobals();
    useWorkspaceStore.setState(before);
  }
});

it.each([false, true])(
  "preserves a draft made while opening a repository (create=%s) when the user cancels switching",
  async (create) => {
    const before = useWorkspaceStore.getState();
    const tabs = [
      { requestId: "draft", name: "Draft", method: "GET", dirty: true },
    ];
    useWorkspaceStore.setState({ activeCollectionId: "local", tabs: [] });
    vi.stubGlobal("window", {});
    const environments = [{ id: "env", collectionId: "repo", name: "Local" }];
    mockIPC((command) => {
      if (command === "open_repository") {
        useWorkspaceStore.setState({ tabs });
        return { collectionId: "repo", collections: [], tree: [] };
      }
      if (command === "list_environments") return environments;
      if (command === "plugin:dialog|message") return "Cancel";
      throw new Error(`Unexpected IPC command: ${command}`);
    });

    try {
      await expect(
        useWorkspaceStore.getState().openRepository("/repo", create, null),
      ).resolves.toBe(create);
      expect(useWorkspaceStore.getState().tabs).toEqual(tabs);
      expect(useWorkspaceStore.getState().activeCollectionId).toBe("local");
      expect(useWorkspaceStore.getState().environments).toEqual(environments);
    } finally {
      clearMocks();
      vi.unstubAllGlobals();
      useWorkspaceStore.setState(before);
    }
  },
);

it("keeps a newly edited draft when switching collections and cancelling", async () => {
  const before = useWorkspaceStore.getState();
  const tabs = [
    { requestId: "draft", name: "Draft", method: "GET", dirty: true },
  ];
  useWorkspaceStore.setState({ activeCollectionId: "local", tabs: [] });
  vi.stubGlobal("window", {});
  mockIPC((command) => {
    if (command === "get_collection_tree") {
      useWorkspaceStore.setState({ tabs });
      return [];
    }
    if (command === "plugin:dialog|message") return "Cancel";
    throw new Error(`Unexpected IPC command: ${command}`);
  });

  try {
    await useWorkspaceStore.getState().selectCollection("other");
    expect(useWorkspaceStore.getState().tabs).toEqual(tabs);
    expect(useWorkspaceStore.getState().activeCollectionId).toBe("local");
  } finally {
    clearMocks();
    vi.unstubAllGlobals();
    useWorkspaceStore.setState(before);
  }
});
