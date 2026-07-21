import { useCallback, useEffect, useState } from "react";
import { AlertTriangle, RefreshCw, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import { api } from "@/lib/tauri";
import type { RequestHistoryEntry } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

const HISTORY_LIMIT = 100;

export function RequestHistoryPanel({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const selectRequest = useWorkspaceStore((state) => state.selectRequest);
  const [entries, setEntries] = useState<RequestHistoryEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await api.listRequestHistory(HISTORY_LIMIT, null);
      setEntries(next);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    void load();
  }, [load, open]);

  useEffect(() => {
    if (!open) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onOpenChange(false);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onOpenChange, open]);

  async function openEntry(entry: RequestHistoryEntry) {
    if (!entry.requestId) return;
    onOpenChange(false);
    await selectRequest(entry.requestId);
  }

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-40 flex justify-end bg-black/40 backdrop-blur-sm"
      onMouseDown={() => onOpenChange(false)}
    >
      <section
        className="flex h-full w-[min(560px,94vw)] min-w-0 flex-col overflow-hidden border-l border-[var(--app-line)] bg-[var(--app-bg)] text-[var(--app-text)] shadow-[-20px_0_60px_rgba(0,0,0,.5)]"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="flex h-11 shrink-0 items-center gap-2 border-b border-[var(--app-line)] bg-[var(--app-panel)] px-3">
          <div className="text-sm font-semibold">Request history</div>
          <span className="text-[11px] text-[var(--app-dim)]">
            Last {HISTORY_LIMIT} · pruned after 3 weeks
          </span>
          <div className="flex-1" />
          <Button
            variant="ghost"
            size="icon"
            className="size-7 text-[var(--app-dim)] hover:text-[var(--app-text)]"
            onClick={() => void load()}
            disabled={loading}
            title="Refresh"
          >
            <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="size-7 text-[var(--app-dim)] hover:text-[var(--app-text)]"
            onClick={() => onOpenChange(false)}
            title="Close"
          >
            <X className="size-3.5" />
          </Button>
        </header>
        <ScrollArea className="min-h-0 flex-1">
          {error ? (
            <div className="mx-4 mt-4 flex items-start gap-2 border border-amber-400/30 bg-amber-400/10 p-3 text-xs text-amber-200">
              <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
              <span className="min-w-0 break-words">{error}</span>
            </div>
          ) : null}
          {!error && !entries.length ? (
            <div className="p-8 text-center text-xs text-[var(--app-dim)]">
              {loading ? "Loading…" : "No requests have been sent yet."}
            </div>
          ) : null}
          <div>
            {entries.map((entry) => (
              <HistoryRow
                key={entry.id}
                entry={entry}
                onSelect={() => void openEntry(entry)}
              />
            ))}
          </div>
        </ScrollArea>
      </section>
    </div>
  );
}

function HistoryRow({
  entry,
  onSelect,
}: {
  entry: RequestHistoryEntry;
  onSelect: () => void;
}) {
  const method = (entry.method || "GET").toUpperCase();
  const statusClass = statusColor(entry.statusCode);
  const clickable = Boolean(entry.requestId);
  const timestamp = formatTimestamp(entry.executedAt);

  return (
    <button
      type="button"
      className={cn(
        "flex w-full items-start gap-3 border-b border-[var(--app-line)] px-3 py-2 text-left text-xs hover:bg-[var(--app-panel-2)]",
        !clickable && "cursor-default hover:bg-transparent",
      )}
      onClick={clickable ? onSelect : undefined}
      title={
        clickable
          ? "Open this request"
          : "The original request has been deleted."
      }
    >
      <span
        className="app-mono mt-0.5 w-12 shrink-0 px-1 py-0.5 text-center text-[10px] font-bold"
        style={methodColor(method)}
      >
        {method}
      </span>
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium text-[var(--app-text)]">
          {entry.name || entry.url || "Untitled"}
        </div>
        <div className="app-mono mt-0.5 truncate text-[10px] text-[var(--app-dim)]">
          {entry.url}
        </div>
        {entry.error ? (
          <div className="mt-1 truncate text-[10px] text-amber-300">
            {entry.error}
          </div>
        ) : null}
      </div>
      <div className="flex shrink-0 flex-col items-end gap-0.5 text-[10px] text-[var(--app-dim)]">
        <span className={cn("font-semibold", statusClass)}>
          {entry.statusCode ?? "—"}
        </span>
        {entry.durationMs != null ? <span>{entry.durationMs} ms</span> : null}
        <span>{timestamp}</span>
      </div>
    </button>
  );
}

function statusColor(status?: number | null): string {
  if (!status) return "text-[var(--app-dim)]";
  if (status >= 500) return "text-red-400";
  if (status >= 400) return "text-amber-300";
  if (status >= 300) return "text-sky-300";
  if (status >= 200) return "text-emerald-300";
  return "text-[var(--app-dim)]";
}

function methodColor(method: string) {
  const normalized = method.toUpperCase();
  const map: Record<string, { color: string; backgroundColor: string }> = {
    GET: { color: "var(--app-get)", backgroundColor: "var(--app-get-bg)" },
    POST: { color: "var(--app-post)", backgroundColor: "var(--app-post-bg)" },
    PUT: { color: "var(--app-put)", backgroundColor: "var(--app-put-bg)" },
    DELETE: {
      color: "var(--app-delete)",
      backgroundColor: "var(--app-delete-bg)",
    },
    PATCH: {
      color: "var(--app-patch)",
      backgroundColor: "var(--app-patch-bg)",
    },
  };
  return {
    ...(map[normalized] ?? {
      color: "var(--app-text)",
      backgroundColor: "transparent",
    }),
    borderRadius: "var(--app-radius)",
  };
}

function formatTimestamp(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  const now = Date.now();
  const delta = now - date.getTime();
  if (delta < 60_000) return "just now";
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m ago`;
  if (delta < 86_400_000) return `${Math.floor(delta / 3_600_000)}h ago`;
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
