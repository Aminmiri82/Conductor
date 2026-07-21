import { type CSSProperties, type DragEvent, useEffect } from "react";
import {
  ChevronDown,
  ChevronRight,
  FileJson,
  Folder,
  FolderOpen,
} from "lucide-react";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { cn } from "@/lib/utils";
import type { CollectionNode } from "@/features/types";
import { useDnD, type DropIntent } from "@/features/collections/treeDnD";

export const TREE_ROW_HEIGHT = 28;

export type TreeRowProps = {
  node: CollectionNode;
  depth: number;
  top: number;
  open: boolean;
  activeRequestId?: string;
  onToggleOpen: (nodeId: string) => void;
  onOpenFolder: (nodeId: string) => void;
  onSelectRequest: (requestId: string) => Promise<void>;
  onCreateRequest: (
    parentId: string | null | undefined,
    position: number,
  ) => Promise<void>;
  onCreateFolder: (
    parentId: string | null | undefined,
    position: number,
  ) => Promise<void>;
  onDuplicateRequest: (requestId: string) => Promise<void>;
  onDeleteRequest: (requestId: string) => Promise<void>;
  onDeleteNode: (node: CollectionNode) => Promise<void>;
  performDrop: () => Promise<void>;
};

export function TreeRow({
  node,
  depth,
  top,
  open,
  activeRequestId,
  onToggleOpen,
  onOpenFolder,
  onSelectRequest,
  onCreateRequest,
  onCreateFolder,
  onDuplicateRequest,
  onDeleteRequest,
  onDeleteNode,
  performDrop,
}: TreeRowProps) {
  const dnd = useDnD();
  const isFolder = node.kind === "folder";
  const isActive = node.requestId === activeRequestId;
  const isDragging = dnd.draggingId === node.id;
  const isInvalidTarget = dnd.draggingDescendantIds.has(node.id);

  const target = dnd.target;
  const targetingMe = target?.kind === "node" && target.nodeId === node.id;
  const showBefore = targetingMe && target.intent === "before";
  const showAfter = targetingMe && target.intent === "after";
  const showInto = targetingMe && target.intent === "into";

  useEffect(() => {
    if (!showInto || !isFolder || open) return;
    const id = window.setTimeout(() => onOpenFolder(node.id), 600);
    return () => window.clearTimeout(id);
  }, [isFolder, node.id, onOpenFolder, open, showInto]);

  const indent = 8 + depth * 13;
  const lineInset = indent + 2;

  function onDragStart(event: DragEvent<HTMLDivElement>) {
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", node.id);
    dnd.beginDrag(node);
  }

  function onDragEnd() {
    dnd.endDrag();
  }

  function onDragOver(event: DragEvent<HTMLDivElement>) {
    if (!dnd.draggingId || isInvalidTarget) return;
    event.preventDefault();
    event.stopPropagation();
    event.dataTransfer.dropEffect = "move";

    const rect = event.currentTarget.getBoundingClientRect();
    const y = event.clientY - rect.top;
    const h = rect.height;

    let intent: DropIntent;
    if (isFolder) {
      if (y < h * 0.3) intent = "before";
      else if (y > h * 0.7) intent = "after";
      else intent = "into";
    } else {
      intent = y < h * 0.5 ? "before" : "after";
    }

    if (
      target?.kind !== "node" ||
      target.nodeId !== node.id ||
      target.intent !== intent
    ) {
      dnd.setTarget({ kind: "node", nodeId: node.id, intent });
    }
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    event.stopPropagation();
    void performDrop();
  }

  return (
    <div
      className="absolute left-0 right-0"
      style={{ top, height: TREE_ROW_HEIGHT }}
    >
      {showBefore ? <DropLine inset={lineInset} position="top" /> : null}
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            role="button"
            tabIndex={0}
            data-sidebar-request-id={node.requestId ?? undefined}
            draggable
            onDragStart={onDragStart}
            onDragEnd={onDragEnd}
            onDragOver={onDragOver}
            onDrop={onDrop}
            className={cn(
              "group relative flex h-7 w-full select-none items-center gap-1.5 pr-1 text-left text-xs text-[var(--app-dim)] transition-colors",
              "hover:bg-[var(--app-panel-2)] hover:text-[var(--app-text)]",
              isActive &&
                "bg-[color-mix(in_oklab,var(--app-accent)_14%,transparent)] text-[var(--app-text)] ring-1 ring-[color-mix(in_oklab,var(--app-accent)_28%,transparent)]",
              isDragging && "opacity-40",
              showInto &&
                "bg-[color-mix(in_oklab,var(--app-accent)_10%,transparent)] text-[var(--app-text)] ring-1 ring-[var(--app-accent)]",
            )}
            style={{ paddingLeft: indent, borderRadius: "var(--app-radius)" }}
            onClick={() => {
              if (isFolder) {
                onToggleOpen(node.id);
              } else if (node.requestId) {
                void onSelectRequest(node.requestId);
              }
            }}
          >
            {isFolder ? (
              open ? (
                <ChevronDown className="size-3.5 shrink-0" />
              ) : (
                <ChevronRight className="size-3.5 shrink-0" />
              )
            ) : (
              <span className="w-3.5 shrink-0" />
            )}
            {isFolder ? (
              open ? (
                <FolderOpen className="size-3.5 shrink-0 text-[var(--app-accent)] opacity-80" />
              ) : (
                <Folder className="size-3.5 shrink-0 text-[var(--app-accent)] opacity-70" />
              )
            ) : (
              <FileJson className="size-3.5 shrink-0 text-[var(--app-dim)]" />
            )}
            {!isFolder ? (
              <span
                className="app-mono w-10 shrink-0 px-1 py-0.5 text-center text-[10px] font-bold leading-none"
                style={methodColor(node.method ?? "GET")}
              >
                {node.method ?? "GET"}
              </span>
            ) : null}
            <span className="min-w-0 flex-1 truncate">{node.name}</span>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-44">
          {isFolder ? (
            <>
              <ContextMenuItem
                onSelect={() =>
                  void onCreateRequest(node.id, node.children.length)
                }
              >
                Add request
              </ContextMenuItem>
              <ContextMenuItem
                onSelect={() =>
                  void onCreateFolder(node.id, node.children.length)
                }
              >
                Add folder
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem
                variant="destructive"
                onSelect={() => void onDeleteNode(node)}
              >
                Delete folder
              </ContextMenuItem>
            </>
          ) : node.requestId ? (
            <>
              <ContextMenuItem
                onSelect={() => void onDuplicateRequest(node.requestId!)}
              >
                Duplicate request
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem
                variant="destructive"
                onSelect={() => void onDeleteRequest(node.requestId!)}
              >
                Delete request
              </ContextMenuItem>
            </>
          ) : null}
        </ContextMenuContent>
      </ContextMenu>
      {showAfter ? <DropLine inset={lineInset} position="bottom" /> : null}
    </div>
  );
}

export function DropLine({
  inset,
  position,
}: {
  inset: number;
  position: "top" | "bottom";
}) {
  return (
    <div
      aria-hidden
      className={cn(
        "pointer-events-none absolute right-2 z-20 h-0.5 rounded-full bg-[var(--app-accent)]",
        position === "top" ? "-top-px" : "-bottom-px",
      )}
      style={{ left: inset }}
    >
      <span className="absolute -left-1 top-1/2 block size-2 -translate-y-1/2 rounded-full bg-[var(--app-accent)] ring-2 ring-[color-mix(in_oklab,var(--app-accent)_25%,transparent)]" />
    </div>
  );
}

export function RootEndDropZone({
  top,
  performDrop,
}: {
  top: number;
  performDrop: () => Promise<void>;
}) {
  const dnd = useDnD();
  const dragging = dnd.draggingId;
  const active = dnd.target?.kind === "root-end";

  if (!dragging) {
    return (
      <div
        aria-hidden
        className="absolute h-2"
        style={{ top, left: 0, right: 0 }}
      />
    );
  }

  return (
    <div
      onDragOver={(event) => {
        event.preventDefault();
        event.stopPropagation();
        event.dataTransfer.dropEffect = "move";
        if (!active) dnd.setTarget({ kind: "root-end" });
      }}
      onDrop={(event) => {
        event.preventDefault();
        event.stopPropagation();
        void performDrop();
      }}
      className={cn(
        "absolute mx-1 mt-1 flex h-7 items-center justify-center border border-dashed text-[10px] uppercase tracking-[0.08em] transition-colors",
        active
          ? "border-[var(--app-accent)] bg-[color-mix(in_oklab,var(--app-accent)_10%,transparent)] text-[var(--app-accent)]"
          : "border-[var(--app-line)] text-[var(--app-dim)]/60",
      )}
      style={{ top, left: 0, right: 0, borderRadius: "var(--app-radius)" }}
    >
      Move to root
    </div>
  );
}

function methodColor(method: string): CSSProperties {
  const normalized = method.toUpperCase();
  const map: Record<string, CSSProperties> = {
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
