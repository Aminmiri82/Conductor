import { Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { KeyValue } from "@/features/types";

export function KeyValueTable({
  rows,
  onChange,
  placeholder,
}: {
  rows: KeyValue[];
  onChange: (rows: KeyValue[]) => void;
  placeholder: string;
}) {
  function update(index: number, patch: Partial<KeyValue>) {
    onChange(rows.map((row, i) => (i === index ? { ...row, ...patch } : row)));
  }

  function remove(index: number) {
    onChange(rows.filter((_, i) => i !== index));
  }

  return (
    <div
      className="overflow-hidden border bg-[var(--app-panel-2)]"
      style={{
        borderColor: "var(--app-line)",
        borderRadius: "var(--app-radius-lg)",
      }}
    >
      <div className="app-mono grid h-8 grid-cols-[34px_minmax(120px,0.8fr)_minmax(160px,1.2fr)_34px] items-center border-b border-[var(--app-line)] bg-[rgb(255_255_255/.02)] px-1 text-[11px] uppercase tracking-[0.06em] text-[var(--app-dim)]">
        <div />
        <div>Key</div>
        <div>Value</div>
        <div />
      </div>
      {rows.map((row, index) => (
        <div
          key={index}
          className="grid grid-cols-[34px_minmax(120px,0.8fr)_minmax(160px,1.2fr)_34px] items-center border-b border-[var(--app-line)] px-1 last:border-b-0"
        >
          <input
            type="checkbox"
            className="mx-auto size-3 accent-[var(--app-accent)]"
            checked={row.enabled}
            onChange={(event) => update(index, { enabled: event.target.checked })}
          />
          <Input
            className="app-mono h-8 rounded-none border-0 bg-transparent text-xs shadow-none focus-visible:ring-0"
            value={row.key}
            placeholder={placeholder}
            onChange={(event) => update(index, { key: event.target.value })}
          />
          <Input
            className="app-mono h-8 rounded-none border-0 bg-transparent text-xs shadow-none focus-visible:ring-0"
            value={row.value}
            placeholder="Value"
            onChange={(event) => update(index, { value: event.target.value })}
          />
          <Button
            variant="ghost"
            size="icon"
            className="size-7 text-[var(--app-dim)]"
            onClick={() => remove(index)}
          >
            <Trash2 className="size-3.5" />
          </Button>
        </div>
      ))}
      <div className="p-1">
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1.5 text-xs text-[var(--app-dim)]"
          onClick={() => onChange([...rows, { key: "", value: "", enabled: true }])}
        >
          <Plus className="size-3.5" />
          Add row
        </Button>
      </div>
    </div>
  );
}
