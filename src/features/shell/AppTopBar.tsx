import { PanelLeftClose, PanelLeftOpen, Settings } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";

export function AppTopBar({ onOpenSettings }: { onOpenSettings: () => void }) {
  const sidebarVisible = useWorkspaceStore((state) => state.sidebarVisible);
  const toggleSidebar = useWorkspaceStore((state) => state.toggleSidebar);
  const appTheme = useWorkspaceUiStore((state) => state.workspaceUi.appTheme);
  const environments = useWorkspaceStore((state) => state.environments);
  const activeEnvironmentId = useWorkspaceUiStore(
    (state) => state.workspaceUi.activeEnvironmentId,
  );
  const selectEnvironment = useWorkspaceStore((state) => state.selectEnvironment);
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
      <Select
        value={activeEnvironmentId ?? "__none__"}
        onValueChange={(value) =>
          void selectEnvironment(value === "__none__" ? null : value)
        }
      >
        <SelectTrigger className="h-8 w-48 border-[var(--app-line)] bg-[var(--app-panel-2)] text-xs">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="__none__">No environment</SelectItem>
          {environments.map((environment) => (
            <SelectItem key={environment.id} value={environment.id}>
              {environment.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
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
