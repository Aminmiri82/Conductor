import { useState } from "react";
import { AlertTriangle, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/tauri";
import { notifyHistoryChanged } from "@/features/history/historyEvents";
import { SectionTitle } from "@/features/settings/settingsShared";

export function DataPane() {
  const [purging, setPurging] = useState(false);
  const [purgedCount, setPurgedCount] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function purgeHistory() {
    const confirmed = window.confirm(
      "Delete all request history entries? This cannot be undone.",
    );
    if (!confirmed) return;
    setPurging(true);
    setError(null);
    try {
      const count = await api.purgeRequestHistory();
      setPurgedCount(count);
      notifyHistoryChanged();
    } catch (caught) {
      setError(String(caught));
    } finally {
      setPurging(false);
    }
  }

  return (
    <div>
      <SectionTitle
        title="Data"
        sub="Manage local storage. Conductor keeps everything on this device."
      />

      <div
        className="mb-4 border bg-[var(--app-panel)] p-4"
        style={{
          borderColor: "var(--app-line)",
          borderRadius: "var(--app-radius-lg)",
        }}
      >
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0 flex-1">
            <div className="text-sm font-medium text-[var(--app-text)]">
              Request history
            </div>
            <div className="mt-1 text-xs leading-5 text-[var(--app-dim)]">
              Every send is logged locally so you can revisit past requests.
              Entries older than 3 weeks are pruned automatically.
            </div>
            {purgedCount !== null ? (
              <div className="mt-2 text-xs text-emerald-300">
                Removed {purgedCount} {purgedCount === 1 ? "entry" : "entries"}.
              </div>
            ) : null}
            {error ? (
              <div className="mt-2 flex items-start gap-1.5 text-xs text-amber-300">
                <AlertTriangle className="mt-0.5 size-3" />
                <span className="min-w-0 break-words">{error}</span>
              </div>
            ) : null}
          </div>
          <Button
            variant="ghost"
            className="h-8 gap-1.5 border border-[var(--app-line)] px-3 text-xs text-[var(--app-dim)] hover:text-destructive"
            onClick={() => void purgeHistory()}
            disabled={purging}
          >
            <Trash2 className="size-3.5" />
            {purging ? "Clearing" : "Clear request history"}
          </Button>
        </div>
      </div>
    </div>
  );
}
