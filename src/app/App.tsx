import { useEffect } from "react";
import { AppShell } from "@/features/shell/AppShell";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";
import { useAppHotkeys } from "@/app/hotkeys";

export default function App() {
  const loadCollections = useWorkspaceStore((state) => state.loadCollections);
  const collections = useWorkspaceStore((state) => state.collections);
  const collectionsLoaded = useWorkspaceStore(
    (state) => state.collectionsLoaded,
  );
  const linkedRepositoriesLoaded =
    collectionsLoaded &&
    collections.some((collection) => collection.repository);
  const refreshRepositories = useWorkspaceStore(
    (state) => state.refreshRepositories,
  );
  const theme = useWorkspaceUiStore((state) => state.workspaceUi.appTheme);
  const accent = useWorkspaceUiStore((state) => state.workspaceUi.accentColor);

  useAppHotkeys();

  useEffect(() => {
    document.documentElement.classList.add("dark");
    void loadCollections();
  }, [loadCollections]);

  useEffect(() => {
    if (!linkedRepositoriesLoaded) return;
    const refresh = () => void refreshRepositories();
    let afterPaint = 0;
    const beforePaint = window.requestAnimationFrame(() => {
      afterPaint = window.requestAnimationFrame(refresh);
    });
    window.addEventListener("focus", refresh);
    return () => {
      window.cancelAnimationFrame(beforePaint);
      window.cancelAnimationFrame(afterPaint);
      window.removeEventListener("focus", refresh);
    };
  }, [linkedRepositoriesLoaded, refreshRepositories]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.setProperty("--app-accent", accent);
  }, [accent, theme]);

  return <AppShell />;
}
