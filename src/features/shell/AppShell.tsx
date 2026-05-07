import { AlertCircle, PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { CollectionSidebar } from "@/features/collections/CollectionSidebar";
import { RequestWorkspace } from "@/features/requests/RequestWorkspace";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

export function AppShell() {
  const sidebarVisible = useWorkspaceStore((state) => state.sidebarVisible);
  const toggleSidebar = useWorkspaceStore((state) => state.toggleSidebar);
  const error = useWorkspaceStore((state) => state.error);
  const clearError = useWorkspaceStore((state) => state.clearError);

  return (
    <main className="h-screen overflow-hidden bg-background text-foreground">
      <div className="flex h-11 items-center border-b border-border/70 bg-[#111014] px-3">
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <Button
            variant="ghost"
            size="icon"
            className="size-7 text-muted-foreground hover:text-foreground"
            onClick={toggleSidebar}
            title="Toggle sidebar"
          >
            {sidebarVisible ? <PanelLeftClose /> : <PanelLeftOpen />}
          </Button>
          <div className="truncate text-sm font-medium tracking-[-0.01em]">
            Conductor
          </div>
        </div>
      </div>

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
              <ResizableHandle className="bg-border/70" />
            </>
          ) : null}
          <ResizablePanel minSize="420px">
            <RequestWorkspace />
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>
    </main>
  );
}
