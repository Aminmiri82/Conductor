import { useEffect } from "react";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";
import { AboutPane } from "@/features/settings/AboutPane";
import { AppearancePane } from "@/features/settings/AppearancePane";
import { DataPane } from "@/features/settings/DataPane";
import { ShortcutsPane } from "@/features/settings/ShortcutsPane";
import { VariablesPane } from "@/features/settings/VariablesPane";
import { SETTINGS_TABS } from "@/features/settings/settingsShared";

export function AppSettings({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const workspaceUi = useWorkspaceUiStore((state) => state.workspaceUi);
  const setWorkspacePreference = useWorkspaceUiStore(
    (state) => state.setWorkspacePreference,
  );
  const activeCollectionId = useWorkspaceStore((state) => state.activeCollectionId);

  useEffect(() => {
    if (!open) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onOpenChange(false);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onOpenChange, open]);

  if (!open) return null;

  const tab = workspaceUi.settingsTab;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-4 backdrop-blur-sm"
      onMouseDown={() => onOpenChange(false)}
    >
      <section
        className="flex h-[min(640px,88vh)] w-[min(960px,94vw)] min-w-0 flex-col overflow-hidden border border-[var(--app-line)] bg-[var(--app-bg)] text-[var(--app-text)] shadow-[0_30px_80px_rgba(0,0,0,.6)]"
        style={{ borderRadius: "var(--app-radius-lg)" }}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="flex h-11 shrink-0 items-center gap-3 border-b border-[var(--app-line)] bg-[var(--app-panel)] px-3">
          <div className="text-sm font-semibold">Settings</div>
          <kbd className="app-mono rounded border border-[var(--app-line)] bg-[var(--app-panel-2)] px-1.5 py-0.5 text-[11px] text-[var(--app-dim)]">
            ⌘,
          </kbd>
          <div className="flex-1" />
          <Button
            variant="ghost"
            size="icon"
            className="size-7 text-[var(--app-dim)] hover:text-[var(--app-text)]"
            onClick={() => onOpenChange(false)}
          >
            <X className="size-3.5" />
          </Button>
        </div>
        <div className="flex min-h-0 flex-1">
          <nav className="w-52 shrink-0 border-r border-[var(--app-line)] bg-[var(--app-panel)] p-2">
            {SETTINGS_TABS.map((item) => (
              <button
                key={item.id}
                className={cn(
                  "mb-1 flex h-8 w-full items-center px-2.5 text-left text-xs font-medium text-[var(--app-dim)] hover:bg-[var(--app-panel-2)] hover:text-[var(--app-text)]",
                  tab === item.id &&
                    "border border-[color-mix(in_oklab,var(--app-accent)_28%,transparent)] bg-[color-mix(in_oklab,var(--app-accent)_14%,transparent)] text-[var(--app-text)]",
                )}
                style={{ borderRadius: "var(--app-radius)" }}
                onClick={() => setWorkspacePreference("settingsTab", item.id)}
              >
                {item.label}
              </button>
            ))}
          </nav>
          <ScrollArea className="min-w-0 flex-1">
            <div className="p-6">
              {tab === "appearance" ? <AppearancePane /> : null}
              {tab === "variables" ? (
                <VariablesPane activeCollectionId={activeCollectionId} />
              ) : null}
              {tab === "shortcuts" ? <ShortcutsPane /> : null}
              {tab === "data" ? <DataPane /> : null}
              {tab === "about" ? <AboutPane /> : null}
            </div>
          </ScrollArea>
        </div>
      </section>
    </div>
  );
}
