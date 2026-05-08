import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import type { CollectionNode } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

type AppMenuAction =
  | "new-request"
  | "duplicate-request"
  | "save-request"
  | "send-request"
  | "close-request"
  | "toggle-sidebar"
  | "focus-url";

export function useAppHotkeys() {
  const toggleSidebar = useWorkspaceStore((state) => state.toggleSidebar);
  const saveActiveRequest = useWorkspaceStore((state) => state.saveActiveRequest);
  const sendActiveRequest = useWorkspaceStore((state) => state.sendActiveRequest);
  const closeRequestTab = useWorkspaceStore((state) => state.closeRequestTab);
  const createRequestIn = useWorkspaceStore((state) => state.createRequestIn);
  const duplicateRequest = useWorkspaceStore((state) => state.duplicateRequest);
  const activeRequestId = useWorkspaceStore((state) => state.activeRequestId);
  const tree = useWorkspaceStore((state) => state.tree);

  useEffect(() => {
    function performAction(action: AppMenuAction) {
      if (action === "toggle-sidebar") {
        toggleSidebar();
      }
      if (action === "save-request") {
        void saveActiveRequest();
      }
      if (action === "duplicate-request" && activeRequestId) {
        void duplicateRequest(activeRequestId);
      }
      if (action === "new-request") {
        const activeNode = activeRequestId
          ? findRequestNode(tree, activeRequestId)
          : undefined;
        void createRequestIn(
          activeNode?.parentId ?? null,
          activeNode ? activeNode.position + 1 : tree.length,
        );
      }
      if (action === "close-request" && activeRequestId) {
        void closeRequestTab(activeRequestId);
      }
      if (action === "send-request") {
        void sendActiveRequest();
      }
      if (action === "focus-url") {
        document.querySelector<HTMLInputElement>("[data-url-input]")?.focus();
      }
    }

    function onKeyDown(event: KeyboardEvent) {
      const mod = event.metaKey || event.ctrlKey;
      if (!mod) return;

      const key = event.key.toLowerCase();
      if (key === "b") {
        event.preventDefault();
        performAction("toggle-sidebar");
      }
      if (key === "s") {
        event.preventDefault();
        performAction("save-request");
      }
      if (key === "d" && activeRequestId) {
        event.preventDefault();
        performAction("duplicate-request");
      }
      if (key === "n") {
        event.preventDefault();
        performAction("new-request");
      }
      if (key === "w" && activeRequestId) {
        event.preventDefault();
        performAction("close-request");
      }
      if (key === "enter") {
        event.preventDefault();
        performAction("send-request");
      }
      if (key === "l") {
        event.preventDefault();
        performAction("focus-url");
      }
    }

    window.addEventListener("keydown", onKeyDown);
    const unlisten = listen<AppMenuAction>("app-menu-action", (event) => {
      performAction(event.payload);
    });

    return () => {
      window.removeEventListener("keydown", onKeyDown);
      void unlisten.then((dispose) => dispose());
    };
  }, [
    activeRequestId,
    closeRequestTab,
    createRequestIn,
    duplicateRequest,
    saveActiveRequest,
    sendActiveRequest,
    toggleSidebar,
    tree,
  ]);
}

function findRequestNode(
  nodes: CollectionNode[],
  requestId: string,
): CollectionNode | undefined {
  for (const node of nodes) {
    if (node.requestId === requestId) return node;
    const child = findRequestNode(node.children, requestId);
    if (child) return child;
  }
  return undefined;
}
