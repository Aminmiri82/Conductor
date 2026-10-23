import { useEffect } from "react";
import { events, type AppMenuAction } from "@/bindings";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

// Reads the store when an action fires instead of subscribing, so the root
// component does not re-render (and re-register the menu listener) on every
// tab switch or tree change.
export function useAppHotkeys() {
  useEffect(() => {
    function performAction(action: AppMenuAction) {
      const {
        activeRequestId,
        closeRequestTab,
        createFolderNextToActive,
        createRequestNextToActive,
        duplicateRequest,
        saveActiveRequest,
        selectRequest,
        sendActiveRequest,
        toggleSidebar,
      } = useWorkspaceStore.getState();
      if (action === "toggle-sidebar") {
        toggleSidebar();
      }
      if (action === "open-request") {
        const focusedRequestId = focusedSidebarRequestId();
        if (focusedRequestId) {
          void selectRequest(focusedRequestId);
        } else {
          window.dispatchEvent(new Event("conductor:open-request"));
        }
      }
      if (action === "settings") {
        window.dispatchEvent(new Event("conductor:open-settings"));
      }
      if (action === "check-for-updates") {
        window.dispatchEvent(new Event("conductor:check-for-updates"));
      }
      if (action === "save-request") {
        void saveActiveRequest();
      }
      // An open editor with its own rows (the Variables settings pane)
      // claims ⌘N, ⌘⇧N and ⌘D by cancelling these events.
      if (
        action === "duplicate-request" &&
        unclaimed("conductor:duplicate") &&
        activeRequestId
      ) {
        void duplicateRequest(activeRequestId);
      }
      if (action === "new-request" && unclaimed("conductor:new")) {
        void createRequestNextToActive();
      }
      if (action === "new-folder" && unclaimed("conductor:new-folder")) {
        void createFolderNextToActive();
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
      const { activeRequestId } = useWorkspaceStore.getState();
      // ⌘⇧D belongs to the Variables settings pane (duplicate environment).
      if (key === "d" && !event.shiftKey) {
        event.preventDefault();
        performAction("duplicate-request");
      }
      if (key === "n") {
        event.preventDefault();
        performAction(event.shiftKey ? "new-folder" : "new-request");
      }
      if (key === "o") {
        event.preventDefault();
        performAction("open-request");
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
      if (key === ",") {
        event.preventDefault();
        window.dispatchEvent(new Event("conductor:open-settings"));
      }
    }

    window.addEventListener("keydown", onKeyDown);
    const unlisten = isTauriRuntime()
      ? events.appMenuAction.listen((event) => {
          performAction(event.payload);
        })
      : Promise.resolve(() => undefined);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
      void unlisten.then((dispose) => dispose());
    };
  }, []);
}

function unclaimed(name: string) {
  return window.dispatchEvent(new Event(name, { cancelable: true }));
}

function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

function focusedSidebarRequestId() {
  const active = document.activeElement;
  if (!(active instanceof HTMLElement)) return undefined;
  return active.closest<HTMLElement>("[data-sidebar-request-id]")?.dataset
    .sidebarRequestId;
}
