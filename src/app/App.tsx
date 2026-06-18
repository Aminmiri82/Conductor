import { useEffect } from "react";
import { AppShell } from "@/features/shell/AppShell";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";
import { useAppHotkeys } from "@/app/hotkeys";

export default function App() {
  const loadCollections = useWorkspaceStore((state) => state.loadCollections);
  const theme = useWorkspaceUiStore((state) => state.workspaceUi.appTheme);
  const accent = useWorkspaceUiStore((state) => state.workspaceUi.accentColor);

  useAppHotkeys();

  useEffect(() => {
    document.documentElement.classList.add("dark");
    void loadCollections();
  }, [loadCollections]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.setProperty("--app-accent", accent);
  }, [accent, theme]);

  return <AppShell />;
}
