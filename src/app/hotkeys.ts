import { useEffect } from "react";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

export function useAppHotkeys() {
  const toggleSidebar = useWorkspaceStore((state) => state.toggleSidebar);
  const saveActiveRequest = useWorkspaceStore((state) => state.saveActiveRequest);
  const sendActiveRequest = useWorkspaceStore((state) => state.sendActiveRequest);
  const closeRequestTab = useWorkspaceStore((state) => state.closeRequestTab);
  const activeRequestId = useWorkspaceStore((state) => state.activeRequestId);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const mod = event.metaKey || event.ctrlKey;
      if (!mod) return;

      const key = event.key.toLowerCase();
      if (key === "b") {
        event.preventDefault();
        toggleSidebar();
      }
      if (key === "s") {
        event.preventDefault();
        void saveActiveRequest();
      }
      if (key === "w" && activeRequestId) {
        event.preventDefault();
        void closeRequestTab(activeRequestId);
      }
      if (key === "enter") {
        event.preventDefault();
        void sendActiveRequest();
      }
      if (key === "l") {
        event.preventDefault();
        document.querySelector<HTMLInputElement>("[data-url-input]")?.focus();
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [activeRequestId, closeRequestTab, saveActiveRequest, sendActiveRequest, toggleSidebar]);
}
