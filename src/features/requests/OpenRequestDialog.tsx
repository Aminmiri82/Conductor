import { FileJson } from "lucide-react";
import { useMemo } from "react";
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import type { CollectionNode } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

type RequestOption = {
  id: string;
  name: string;
  method: string;
  path: string;
};

export function OpenRequestDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const tree = useWorkspaceStore((state) => state.tree);
  const selectRequest = useWorkspaceStore((state) => state.selectRequest);
  const requests = useMemo(() => flattenRequests(tree), [tree]);

  return (
    <CommandDialog
      open={open}
      onOpenChange={onOpenChange}
      title="Open Request"
      description="Search requests in the active collection."
      className="border border-[var(--app-line)] bg-[var(--app-panel)] text-[var(--app-text)] sm:max-w-2xl"
    >
      <Command className="bg-[var(--app-panel)]">
        <CommandInput placeholder="Open request..." />
        <CommandList className="max-h-[420px]">
          <CommandEmpty>No requests found.</CommandEmpty>
          <CommandGroup heading="Requests">
            {requests.map((request) => (
              <CommandItem
                key={request.id}
                value={`${request.method} ${request.path} ${request.name}`}
                onSelect={() => {
                  onOpenChange(false);
                  void selectRequest(request.id);
                }}
              >
                <FileJson className="size-4 text-[var(--app-dim)]" />
                <span
                  className="app-mono shrink-0 px-1.5 py-0.5 text-[10px] font-bold"
                  style={methodColor(request.method)}
                >
                  {request.method}
                </span>
                <span className="min-w-0 flex-1 truncate">{request.name}</span>
                <span className="app-mono min-w-0 max-w-64 truncate text-[11px] text-[var(--app-dim)]">
                  {request.path}
                </span>
              </CommandItem>
            ))}
          </CommandGroup>
        </CommandList>
      </Command>
    </CommandDialog>
  );
}

function flattenRequests(
  nodes: CollectionNode[],
  parentPath = "",
): RequestOption[] {
  return nodes.flatMap((node) => {
    const path = parentPath ? `${parentPath} / ${node.name}` : node.name;
    if (node.kind === "request" && node.requestId) {
      return [
        {
          id: node.requestId,
          name: node.name,
          method: node.method ?? "GET",
          path,
        },
      ];
    }
    return flattenRequests(node.children, path);
  });
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
