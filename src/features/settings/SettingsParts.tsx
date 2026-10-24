import { Fragment, type ReactNode, useRef } from "react";
import { Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
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

export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: readonly (readonly [T, string])[];
  onChange: (value: T) => void;
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

export const drawerAction =
  "size-6 text-[var(--app-dim)] hover:text-[var(--app-text)]";

export const panelFrame =
  "border border-[var(--app-line)] bg-[var(--app-panel)]";
export const panelLabel =
  "text-[10px] font-semibold uppercase tracking-[0.08em] text-[var(--app-dim)]";

type ListDrawerItem = {
  id: string;
  name: string;
  // A dot before the name, e.g. the collection open in the sidebar.
  markTitle?: string;
  aside?: ReactNode;
};

// A right-click action on a drawer row. It runs once the menu has closed, so
// focus can move (to a name field, a prompt) without the menu taking it back.
type ListDrawerAction = {
  label: string;
  icon: ReactNode;
  destructive?: boolean;
  run: (id: string) => void;
};

// The list beside a settings page's detail: pick one item, or add one.
export function ListDrawer({
  title,
  items,
  selectedId,
  onSelect,
  createTitle,
  onCreate,
  actions,
  itemActions,
  editingId,
  editLabel,
  onFinishEdit,
}: {
  title: string;
  items: ListDrawerItem[];
  selectedId: string | undefined;
  onSelect: (id: string) => void;
  createTitle?: string;
  onCreate?: () => void;
  // More header buttons, placed before the create button.
  actions?: ReactNode;
  itemActions?: ListDrawerAction[];
  // The row whose name is being edited in place.
  editingId?: string;
  editLabel?: string;
  // `null` cancels the edit.
  onFinishEdit?: (id: string, name: string | null) => void;
}) {
  const pending = useRef<(() => void) | null>(null);
  // Escape unmounts the name field, and the blur that follows must not save.
  const cancelled = useRef(false);
  return (
    <div
      className={cn("self-start overflow-hidden", panelFrame)}
      style={{ borderRadius: "var(--app-radius-lg)" }}
    >
      <div className="flex h-8 items-center justify-between border-b border-[var(--app-line)] bg-[rgb(255_255_255/.02)] pr-1 pl-2.5">
        <span className={panelLabel}>{title}</span>
        <span className="flex items-center">
          {actions}
          {onCreate ? (
            <Button
              variant="ghost"
              size="icon"
              className={drawerAction}
              title={createTitle}
              aria-label={createTitle}
              onClick={onCreate}
            >
              <Plus className="size-3.5" />
            </Button>
          ) : null}
        </span>
      </div>
      {items.map((item) => {
        const selected = item.id === selectedId;
        const rowClass = cn(
          "flex h-8 w-full items-center gap-2 border-l-2 border-transparent px-2.5 text-left text-xs text-[var(--app-dim)]",
          selected &&
            "border-[var(--app-accent)] bg-[color-mix(in_oklab,var(--app-accent)_12%,transparent)] text-[var(--app-text)]",
        );
        const mark = item.markTitle ? (
          <span
            className="size-1.5 shrink-0 rounded-full bg-[var(--app-accent)]"
            title={item.markTitle}
          />
        ) : null;
        if (item.id === editingId) {
          return (
            <div key={item.id} className={rowClass}>
              {mark}
              <input
                autoFocus
                defaultValue={item.name}
                aria-label={editLabel}
                className="-mx-1 h-6 min-w-0 flex-1 border border-[var(--app-accent)] bg-[var(--app-panel-2)] px-1 text-xs text-[var(--app-text)] outline-none"
                style={{ borderRadius: "var(--app-radius)" }}
                onFocus={(event) => {
                  cancelled.current = false;
                  event.currentTarget.select();
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") event.currentTarget.blur();
                  if (event.key === "Escape") {
                    cancelled.current = true;
                    onFinishEdit?.(item.id, null);
                  }
                }}
                onBlur={(event) => {
                  if (!cancelled.current) {
                    onFinishEdit?.(item.id, event.currentTarget.value);
                  }
                }}
                data-cancel-on-escape
              />
            </div>
          );
        }
        const row = (
          <button
            key={item.id}
            className={cn(
              rowClass,
              "hover:bg-[var(--app-panel-2)] hover:text-[var(--app-text)]",
            )}
            aria-current={selected ? "true" : undefined}
            onClick={() => onSelect(item.id)}
          >
            {mark}
            <span className="min-w-0 flex-1 truncate">{item.name}</span>
            {item.aside !== undefined ? (
              <span className="app-mono text-[11px] opacity-70">
                {item.aside}
              </span>
            ) : null}
          </button>
        );
        if (!itemActions?.length) return row;
        return (
          <ContextMenu key={item.id}>
            <ContextMenuTrigger asChild>{row}</ContextMenuTrigger>
            <ContextMenuContent
              className="w-48"
              onCloseAutoFocus={(event) => {
                event.preventDefault();
                const run = pending.current;
                pending.current = null;
                run?.();
              }}
            >
              {itemActions.map((action) => (
                <Fragment key={action.label}>
                  {action.destructive ? <ContextMenuSeparator /> : null}
                  <ContextMenuItem
                    variant={action.destructive ? "destructive" : "default"}
                    onSelect={() => {
                      pending.current = () => action.run(item.id);
                    }}
                  >
                    {action.icon}
                    {action.label}
                  </ContextMenuItem>
                </Fragment>
              ))}
            </ContextMenuContent>
          </ContextMenu>
        );
      })}
    </div>
  );
}

const titleClass =
  "min-w-0 flex-1 truncate border border-transparent px-2 text-left text-base font-semibold text-[var(--app-text)]";

// A detail page's heading. With `onStartEdit` the name is the rename control:
// click it to edit. Place it in a `-ml-2` row so the text lines up.
export function EditableTitle({
  name,
  label,
  editing = false,
  onStartEdit,
  onFinishEdit,
}: {
  name: string;
  label: string;
  editing?: boolean;
  onStartEdit?: () => void;
  // `null` cancels the edit.
  onFinishEdit?: (name: string | null) => void;
}) {
  // Escape unmounts the field, and the blur that follows must not save.
  const cancelled = useRef(false);
  if (editing) {
    return (
      <input
        autoFocus
        defaultValue={name}
        aria-label={label}
        className="h-8 min-w-0 flex-1 border border-[var(--app-accent)] bg-[var(--app-panel-2)] px-2 text-base font-semibold text-[var(--app-text)] outline-none"
        style={{ borderRadius: "var(--app-radius)" }}
        onFocus={(event) => {
          cancelled.current = false;
          event.currentTarget.select();
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
          if (event.key === "Escape") {
            cancelled.current = true;
            onFinishEdit?.(null);
          }
        }}
        onBlur={(event) => {
          if (!cancelled.current) onFinishEdit?.(event.currentTarget.value);
        }}
        data-cancel-on-escape
      />
    );
  }
  if (!onStartEdit) return <div className={titleClass}>{name}</div>;
  return (
    <button
      className={cn(
        titleClass,
        "cursor-text hover:border-[var(--app-line)] hover:bg-[var(--app-panel-2)]",
      )}
      style={{ borderRadius: "var(--app-radius)" }}
      title="Click to rename"
      onClick={onStartEdit}
    >
      {name}
    </button>
  );
}
