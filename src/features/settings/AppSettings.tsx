import { useRef } from "react";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import type { SettingsTab } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";
import { AboutPane } from "./AboutPane";
import { AppearancePane } from "./AppearancePane";
import { ShortcutsPane } from "./ShortcutsPane";
import { VariablesPane } from "./VariablesPane";

const SETTINGS_TABS: { id: SettingsTab; label: string }[] = [
  { id: "appearance", label: "Appearance" },
  { id: "variables", label: "Variables" },
  { id: "shortcuts", label: "Shortcuts" },
  { id: "about", label: "About" },
];

export function AppSettings({
  open,
  onOpenChange,
  updateCheckRequested,
  consumeUpdateCheck,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  updateCheckRequested: boolean;
  consumeUpdateCheck: () => void;
}) {
  const tab = useWorkspaceUiStore((state) => state.workspaceUi.settingsTab);
  const setWorkspacePreference = useWorkspaceUiStore(
    (state) => state.setWorkspacePreference,
  );
  const activeCollectionId = useWorkspaceStore(
    (state) => state.activeCollectionId,
  );
  const contentRef = useRef<HTMLDivElement>(null);
  // Opened from a menu or shortcut, not a Radix trigger, so Radix has nothing
  // to return focus to; put it back where the user was.
  const returnFocusTo = useRef<HTMLElement | null>(null);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        ref={contentRef}
        showCloseButton={false}
        aria-describedby={undefined}
        overlayClassName="bg-black/45 supports-backdrop-filter:backdrop-blur-sm"
        // Land focus on the dialog itself rather than ringing the close button.
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          returnFocusTo.current =
            document.activeElement instanceof HTMLElement
              ? document.activeElement
              : null;
          contentRef.current?.focus();
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          const target = returnFocusTo.current;
          // In chip and syntax modes the URL input exists only while editing,
          // so it is gone by now; land on the URL bar's display instead.
          if (
            target &&
            !target.isConnected &&
            target.matches("[data-url-input]")
          ) {
            document.querySelector<HTMLElement>("[data-url-input]")?.focus();
          } else {
            target?.focus();
          }
        }}
        className="flex h-[min(640px,88vh)] w-[min(960px,94vw)] max-w-none min-w-0 flex-col gap-0 overflow-hidden border border-[var(--app-line)] bg-[var(--app-bg)] p-0 text-base shadow-[0_30px_80px_rgba(0,0,0,.6)] ring-0 sm:max-w-none"
        style={{ borderRadius: "var(--app-radius-lg)" }}
      >
        <div className="flex h-11 shrink-0 items-center gap-3 border-b border-[var(--app-line)] bg-[var(--app-panel)] px-3">
          <DialogTitle className="font-[family-name:inherit] text-sm font-semibold">
            Settings
          </DialogTitle>
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
              {tab === "about" ? (
                <AboutPane
                  updateCheckRequested={updateCheckRequested}
                  consumeUpdateCheck={consumeUpdateCheck}
                />
              ) : null}
            </div>
          </ScrollArea>
        </div>
      </DialogContent>
    </Dialog>
  );
}
