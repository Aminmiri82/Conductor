import { type ReactNode } from "react";
import { cn } from "@/lib/utils";

export function SectionTitle({ title, sub }: { title: string; sub?: string }) {
  return (
    <div className="mb-3">
      <div className="text-sm font-semibold text-[var(--app-text)]">
        {title}
      </div>
      {sub ? (
        <div className="mt-1 text-xs text-[var(--app-dim)]">{sub}</div>
      ) : null}
    </div>
  );
}

export function SettingRow({
  label,
  hint,
  children,
}: {
  label: string;
  hint: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 border-b border-[var(--app-line)] py-4">
      <div>
        <div className="text-sm font-medium text-[var(--app-text)]">
          {label}
        </div>
        <div className="mt-0.5 text-xs text-[var(--app-dim)]">{hint}</div>
      </div>
      {children}
    </div>
  );
}

export function Segmented({
  value,
  options,
  onChange,
}: {
  value: string;
  options: readonly (readonly [string, string])[];
  onChange: (value: string) => void;
}) {
  return (
    <div
      className="flex shrink-0 border bg-[var(--app-panel-2)] p-0.5 text-xs"
      style={{
        borderColor: "var(--app-line)",
        borderRadius: "var(--app-radius)",
      }}
    >
      {options.map(([id, label]) => (
        <button
          key={id}
          className={cn(
            "px-2.5 py-1 font-medium text-[var(--app-dim)]",
            value === id &&
              "bg-[var(--app-accent)] text-[var(--app-accent-fg)]",
          )}
          style={{ borderRadius: "var(--app-radius)" }}
          onClick={() => onChange(id)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
