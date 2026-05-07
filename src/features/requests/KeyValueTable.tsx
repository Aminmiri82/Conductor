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
    <div className="overflow-hidden rounded-md border border-border/70">
      <div className="grid h-8 grid-cols-[34px_minmax(120px,0.8fr)_minmax(160px,1.2fr)_34px] items-center border-b border-border/70 bg-muted/20 px-1 text-xs text-muted-foreground">
        <div />
        <div>Key</div>
        <div>Value</div>
        <div />
      </div>
      {rows.map((row, index) => (
        <div
          key={index}
          className="grid grid-cols-[34px_minmax(120px,0.8fr)_minmax(160px,1.2fr)_34px] items-center border-b border-border/40 px-1 last:border-b-0"
        >
          <input
            type="checkbox"
            className="mx-auto size-3 accent-violet-400"
            checked={row.enabled}
            onChange={(event) => update(index, { enabled: event.target.checked })}
          />
          <Input
            className="h-8 rounded-none border-0 bg-transparent font-mono text-xs shadow-none focus-visible:ring-0"
            value={row.key}
            placeholder={placeholder}
            onChange={(event) => update(index, { key: event.target.value })}
          />
          <Input
            className="h-8 rounded-none border-0 bg-transparent font-mono text-xs shadow-none focus-visible:ring-0"
            value={row.value}
            placeholder="Value"
            onChange={(event) => update(index, { value: event.target.value })}
          />
          <Button
            variant="ghost"
            size="icon"
            className="size-7 text-muted-foreground"
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
          className="h-7 gap-1.5 text-xs text-muted-foreground"
          onClick={() => onChange([...rows, { key: "", value: "", enabled: true }])}
        >
          <Plus className="size-3.5" />
          Add row
        </Button>
      </div>
    </div>
  );
}
