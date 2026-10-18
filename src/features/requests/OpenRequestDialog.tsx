import { FileJson } from "lucide-react";
import { useMemo, useState } from "react";
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  flattenRequests,
  searchRequests,
} from "@/features/requests/requestSearch";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

// Rendering every request of a large collection on each keystroke is what made
// this dialog slow; nobody scrolls past the first screenful of results.
const RESULT_LIMIT = 50;

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
  const [search, setSearch] = useState("");
  const { results, total } = useMemo(
    () => searchRequests(requests, search, RESULT_LIMIT),
    [requests, search],
  );

  return (
    <CommandDialog
      open={open}
      onOpenChange={(open) => {
        if (!open) setSearch("");
        onOpenChange(open);
      }}
      title="Open Request"
      description="Search requests in the active collection."
      className="border border-[var(--app-line)] bg-[var(--app-panel)] text-[var(--app-text)] sm:max-w-2xl"
    >
      <Command className="bg-[var(--app-panel)]" shouldFilter={false}>
        <CommandInput
          placeholder="Open request..."
          value={search}
          onValueChange={setSearch}
        />
        <CommandList className="max-h-[420px]">
          <CommandEmpty>No requests found.</CommandEmpty>
          <CommandGroup heading="Requests">
            {results.map((request) => (
              <CommandItem
                key={request.id}
                value={request.id}
                onSelect={() => {
                  setSearch("");
                  onOpenChange(false);
                  void selectRequest(request.id);
                }}
              >
                <FileJson className="size-4 text-[var(--app-dim)]" />
                <span
                  className="method-badge app-mono shrink-0 px-1.5 py-0.5 text-[10px] font-bold"
                  data-method={request.method}
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
          {total > results.length ? (
            <div className="px-3 py-2 text-xs text-[var(--app-dim)]">
              Showing {results.length} of {total}. Keep typing to narrow.
            </div>
          ) : null}
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
