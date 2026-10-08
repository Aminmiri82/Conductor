import type { ComponentProps, ReactNode } from "react";
import { Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

// The pieces the editable-row tables share (headers, params, form data,
// variables). Columns differ per table, so each table passes its own
// `grid-cols-[...]` class to `HeaderRow` and `Row`; it must be a literal in
// that table's file for Tailwind to see it.

export function TableFrame({
  className,
  children,
}: {
  className?: string;
  children: ReactNode;
}) {
  return (
    <div
      className={cn("overflow-hidden border", className)}
      style={{
        borderColor: "var(--app-line)",
        borderRadius: "var(--app-radius-lg)",
      }}
    >
      {children}
    </div>
  );
}

export function HeaderRow({
  columns,
  className,
  children,
}: {
  columns: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div
      className={cn(
        "grid items-center border-b border-[var(--app-line)] bg-[rgb(255_255_255/.02)] px-1 text-[11px] uppercase tracking-[0.06em] text-[var(--app-dim)]",
        columns,
        className,
      )}
    >
      {children}
    </div>
  );
}

export function Row({
  columns,
  children,
}: {
  columns: string;
  children: ReactNode;
}) {
  return (
    <div
      className={cn(
        "grid items-center border-b border-[var(--app-line)] px-1 last:border-b-0",
        columns,
      )}
    >
      {children}
    </div>
  );
}

export function CheckboxCell({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <input
      type="checkbox"
      className="mx-auto size-3 accent-[var(--app-accent)]"
      checked={checked}
      onChange={(event) => onChange(event.target.checked)}
    />
  );
}

export function CellInput({ className, ...props }: ComponentProps<"input">) {
  return (
    <Input
      className={cn(
        "app-mono h-8 rounded-none border-0 bg-transparent text-xs shadow-none focus-visible:ring-0",
        className,
      )}
      {...props}
    />
  );
}

export function RemoveButton({
  className,
  title,
  onClick,
}: {
  className?: string;
  title?: string;
  onClick: () => void;
}) {
  return (
    <Button
      variant="ghost"
      size="icon"
      className={cn("size-7 text-[var(--app-dim)]", className)}
      onClick={onClick}
      title={title}
    >
      <Trash2 className="size-3.5" />
    </Button>
  );
}

export function AddRowButton({
  onClick,
  children,
}: {
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <div className="p-1">
      <Button
        variant="ghost"
        size="sm"
        className="h-7 gap-1.5 text-xs text-[var(--app-dim)]"
        onClick={onClick}
      >
        <Plus className="size-3.5" />
        {children}
      </Button>
    </div>
  );
}
