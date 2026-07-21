import { useEffect, useState } from "react";
import { api } from "@/lib/tauri";
import type { DatabaseStatus } from "@/features/types";
import { SectionTitle } from "@/features/settings/settingsShared";

export function AboutPane() {
  const [status, setStatus] = useState<DatabaseStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    api
      .databaseStatus()
      .then((next) => {
        if (mounted) setStatus(next);
      })
      .catch((caught) => {
        if (mounted) setError(String(caught));
      });
    return () => {
      mounted = false;
    };
  }, []);

  return (
    <div>
      <SectionTitle
        title="Conductor"
        sub="API workspace for collections and requests."
      />
      <div className="app-mono text-xs leading-6 text-[var(--app-dim)]">
        Version 0.1.0
      </div>

      <div className="mt-6">
        <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.09em] text-[var(--app-dim)]">
          Local storage
        </div>
        <div
          className="overflow-hidden border bg-[var(--app-panel)]"
          style={{
            borderColor: "var(--app-line)",
            borderRadius: "var(--app-radius-lg)",
          }}
        >
          <StatusRow label="Database path" value={status?.path ?? "…"} mono />
          <StatusRow
            label="Schema version"
            value={status ? String(status.schemaVersion) : "…"}
            mono
          />
          <StatusRow
            label="Collections"
            value={status ? String(status.collectionCount) : "…"}
          />
          <StatusRow
            label="Requests"
            value={status ? String(status.requestCount) : "…"}
          />
        </div>
        {error ? (
          <div className="mt-2 text-xs text-amber-300">{error}</div>
        ) : null}
      </div>
    </div>
  );
}

function StatusRow({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="flex items-center justify-between gap-3 border-b border-[var(--app-line)] px-3 py-2 last:border-b-0">
      <div className="text-xs text-[var(--app-dim)]">{label}</div>
      <div
        className={`min-w-0 truncate text-xs text-[var(--app-text)] ${mono ? "app-mono" : ""}`}
        title={value}
      >
        {value}
      </div>
    </div>
  );
}
