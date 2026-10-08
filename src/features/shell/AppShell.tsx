import { lazy, Suspense, useCallback, useEffect, useState } from "react";
import { AlertCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { CollectionSidebar } from "@/features/collections/CollectionSidebar";
import { RequestWorkspace } from "@/features/requests/RequestWorkspace";
import { AppTopBar } from "@/features/shell/AppTopBar";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";

// Neither is on the first screen. Each loads the first time it opens and then
// stays mounted so its close animation can run.
const AppSettings = lazy(() =>
  import("@/features/settings/AppSettings").then((module) => ({
    default: module.AppSettings,
  })),
);
const OpenRequestDialog = lazy(() =>
  import("@/features/requests/OpenRequestDialog").then((module) => ({
    default: module.OpenRequestDialog,
  })),
);

export function AppShell() {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [openRequestOpen, setOpenRequestOpen] = useState(false);
  const [updateCheckRequested, setUpdateCheckRequested] = useState(false);
  const consumeUpdateCheck = useCallback(
    () => setUpdateCheckRequested(false),
    [],
  );
  const settingsOpened = useOpenedOnce(settingsOpen);
  const openRequestOpened = useOpenedOnce(openRequestOpen);
  const sidebarVisible = useWorkspaceStore((state) => state.sidebarVisible);
  const error = useWorkspaceStore((state) => state.error);
  const clearError = useWorkspaceStore((state) => state.clearError);

  useEffect(() => {
    function openSettings() {
      setSettingsOpen(true);
    }
    function openRequest() {
      setOpenRequestOpen(true);
    }
    function checkForUpdates() {
      useWorkspaceUiStore
        .getState()
        .setWorkspacePreference("settingsTab", "about");
      setUpdateCheckRequested(true);
      setSettingsOpen(true);
    }

    window.addEventListener("conductor:open-settings", openSettings);
    window.addEventListener("conductor:open-request", openRequest);
    window.addEventListener("conductor:check-for-updates", checkForUpdates);
    return () => {
      window.removeEventListener("conductor:open-settings", openSettings);
      window.removeEventListener("conductor:open-request", openRequest);
      window.removeEventListener(
        "conductor:check-for-updates",
        checkForUpdates,
      );
    };
  }, []);

  return (
    <main className="flex h-screen flex-col overflow-hidden bg-[var(--app-bg)] text-[var(--app-text)]">
      <AppTopBar onOpenSettings={() => setSettingsOpen(true)} />

      {error ? (
        <div className="flex h-9 shrink-0 items-center gap-2 border-b border-destructive/30 bg-destructive/10 px-3 text-xs text-destructive">
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

      <div className="min-h-0 flex-1">
        <ResizablePanelGroup orientation="horizontal">
          {sidebarVisible ? (
            <>
              <ResizablePanel
                defaultSize="300px"
                minSize="240px"
                maxSize="460px"
              >
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
      <Suspense>
        {openRequestOpened ? (
          <OpenRequestDialog
            open={openRequestOpen}
            onOpenChange={setOpenRequestOpen}
          />
        ) : null}
        {settingsOpened ? (
          <AppSettings
            open={settingsOpen}
            onOpenChange={setSettingsOpen}
            updateCheckRequested={updateCheckRequested}
            consumeUpdateCheck={consumeUpdateCheck}
          />
        ) : null}
      </Suspense>
    </main>
  );
}

function useOpenedOnce(open: boolean) {
  const [opened, setOpened] = useState(open);
  if (open && !opened) setOpened(true);
  return opened;
}
