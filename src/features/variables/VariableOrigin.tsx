import type { VariableEntry } from "@/features/types";

export function VariableOrigin({ origin }: Pick<VariableEntry, "origin">) {
  if (!origin) return null;
  if (origin === "missingSecret") {
    return <span className="text-[11px] text-amber-400">Missing secret</span>;
  }
  return (
    <span className="text-[11px] text-[var(--app-dim)]">
      {origin === "shared" ? "Shared" : "Local"}
    </span>
  );
}
