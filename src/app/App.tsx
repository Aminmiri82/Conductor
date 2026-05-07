import { useEffect } from "react";
import { AppShell } from "@/features/shell/AppShell";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { useAppHotkeys } from "@/app/hotkeys";

export default function App() {
  const loadCollections = useWorkspaceStore((state) => state.loadCollections);

  useAppHotkeys();

  useEffect(() => {
    document.documentElement.classList.add("dark");
    void loadCollections();
  }, [loadCollections]);

  return <AppShell />;
}
