import { PanelLeftClose, PanelLeftOpen, Settings } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

export function AppTopBar({ onOpenSettings }: { onOpenSettings: () => void }) {
  const sidebarVisible = useWorkspaceStore((state) => state.sidebarVisible);
  const toggleSidebar = useWorkspaceStore((state) => state.toggleSidebar);
  const appTheme = useWorkspaceStore((state) => state.workspaceUi.appTheme);
  const isGraphicTheme = appTheme !== "softpro";

  return (
    <div className="flex h-11 items-center gap-3 border-b border-[var(--app-line)] bg-[var(--app-panel)] px-3">
      <div className="flex min-w-0 items-center gap-2">
        <Button
          variant="ghost"
          size="icon"
          className="size-7 text-[var(--app-dim)] hover:text-[var(--app-text)]"
          onClick={toggleSidebar}
          title="Toggle sidebar"
        >
          {sidebarVisible ? <PanelLeftClose /> : <PanelLeftOpen />}
        </Button>
        <div
          className={`truncate text-sm font-semibold ${
            isGraphicTheme ? "app-mono uppercase tracking-[0.08em]" : ""
          }`}
        >
          Conductor
        </div>
      </div>
      <div className="flex-1" />
      <Button
        variant="ghost"
        size="icon"
        className="size-7 border border-[var(--app-line)] text-[var(--app-dim)] hover:text-[var(--app-text)]"
        style={{ borderRadius: "var(--app-radius)" }}
        onClick={onOpenSettings}
        title="Settings"
      >
        <Settings className="size-3.5" />
      </Button>
    </div>
  );
}
