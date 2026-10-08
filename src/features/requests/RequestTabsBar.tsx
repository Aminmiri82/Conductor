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
    <div className="app-scroll flex h-10 min-w-0 items-end overflow-x-auto border-b border-[var(--app-line)] bg-[var(--app-panel)]">
      {tabs.map((tab) => {
        const active = tab.requestId === activeRequestId;
        return (
          <button
            key={tab.requestId}
            className={cn(
              "group relative flex h-9 min-w-36 max-w-60 items-center gap-2 border-r border-[var(--app-line)] px-3 text-left text-xs text-[var(--app-dim)] hover:bg-[var(--app-panel-2)] hover:text-[var(--app-text)]",
              active && "bg-[var(--app-panel-2)] text-[var(--app-text)]",
            )}
            style={{ textTransform: "var(--app-tab-transform)" }}
            onClick={() => void selectRequest(tab.requestId)}
          >
            {active ? (
              <span className="absolute inset-x-0 top-0 h-0.5 bg-[var(--app-accent)]" />
            ) : null}
            <span
              className="method-badge app-mono px-[5px] py-[2px] text-[10px] font-bold"
              data-method={tab.method}
            >
              {tab.method}
            </span>
            <span className="min-w-0 flex-1 truncate">{tab.name}</span>
            <span className="relative grid size-4 shrink-0 place-items-center rounded hover:bg-accent">
              {tab.dirty ? (
                <Circle className="size-2.5 fill-[var(--app-accent)] text-[var(--app-accent)] group-hover:opacity-0" />
              ) : null}
              <X
                className="absolute size-3 opacity-0 group-hover:opacity-100"
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
