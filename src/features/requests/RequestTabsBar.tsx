import { Circle, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

export function RequestTabsBar() {
  const tabs = useWorkspaceStore((state) => state.tabs);
  const activeRequestId = useWorkspaceStore((state) => state.activeRequestId);
  const selectRequest = useWorkspaceStore((state) => state.selectRequest);
  const closeRequestTab = useWorkspaceStore((state) => state.closeRequestTab);

  if (!tabs.length) {
    return null;
  }

  return (
    <div className="flex h-9 min-w-0 items-end overflow-x-auto border-b border-border/70 bg-[#111014] px-2">
      {tabs.map((tab) => {
        const active = tab.requestId === activeRequestId;
        return (
          <button
            key={tab.requestId}
            className={cn(
              "group flex h-8 min-w-36 max-w-56 items-center gap-2 border-r border-border/50 px-3 text-left text-xs text-muted-foreground hover:bg-[#191820] hover:text-foreground",
              active &&
                "border-t border-t-violet-400/80 bg-[#17161d] text-foreground",
            )}
            onClick={() => void selectRequest(tab.requestId)}
          >
            <span className="font-mono text-[10px] text-violet-300/80">
              {tab.method}
            </span>
            <span className="min-w-0 flex-1 truncate">{tab.name}</span>
            <span className="relative grid size-4 shrink-0 place-items-center rounded hover:bg-accent">
              {tab.dirty ? (
                <Circle className="size-2.5 fill-violet-300 text-violet-300 group-hover:opacity-0" />
              ) : null}
              <X
                className={cn(
                  "absolute size-3 opacity-0 group-hover:opacity-100",
                  !tab.dirty && "opacity-0",
                )}
                onClick={(event) => {
                  event.stopPropagation();
                  void closeRequestTab(tab.requestId);
                }}
              />
            </span>
          </button>
        );
      })}
    </div>
  );
}
