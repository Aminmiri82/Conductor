import { useEffect, useState } from "react";
import { AlertCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { CollectionSidebar } from "@/features/collections/CollectionSidebar";
import { RequestWorkspace } from "@/features/requests/RequestWorkspace";
import { AppSettings } from "@/features/settings/AppSettings";
import { AppTopBar } from "@/features/shell/AppTopBar";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

export function AppShell() {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const sidebarVisible = useWorkspaceStore((state) => state.sidebarVisible);
  const error = useWorkspaceStore((state) => state.error);
  const clearError = useWorkspaceStore((state) => state.clearError);

  useEffect(() => {
    function openSettings() {
      setSettingsOpen(true);
    }
    window.addEventListener("conductor:open-settings", openSettings);
    return () =>
      window.removeEventListener("conductor:open-settings", openSettings);
  }, []);

  return (
    <main className="h-screen overflow-hidden bg-[var(--app-bg)] text-[var(--app-text)]">
      <AppTopBar onOpenSettings={() => setSettingsOpen(true)} />

      {error ? (
        <div className="flex h-9 items-center gap-2 border-b border-destructive/30 bg-destructive/10 px-3 text-xs text-destructive">
          <AlertCircle className="size-4" />
          <span className="min-w-0 flex-1 truncate">{error}</span>
          <Button
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-destructive hover:text-destructive"
            onClick={clearError}
          >
            Dismiss
          </Button>
        </div>
      ) : null}

      <div className={error ? "h-[calc(100vh-5rem)]" : "h-[calc(100vh-2.75rem)]"}>
        <ResizablePanelGroup orientation="horizontal">
          {sidebarVisible ? (
            <>
              <ResizablePanel defaultSize="300px" minSize="240px" maxSize="460px">
                <CollectionSidebar />
              </ResizablePanel>
              <ResizableHandle className="bg-[var(--app-line)]" />
            </>
          ) : null}
          <ResizablePanel minSize="420px">
            <RequestWorkspace />
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>
      <AppSettings open={settingsOpen} onOpenChange={setSettingsOpen} />
    </main>
  );
}
