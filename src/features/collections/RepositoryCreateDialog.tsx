import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

export default function RepositoryCreateDialog({
  root,
  onClose,
}: {
  root: string;
  onClose: () => void;
}) {
  const collections = useWorkspaceStore((state) => state.collections);
  const openRepository = useWorkspaceStore((state) => state.openRepository);
  const [source, setSource] = useState("empty");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  async function create() {
    setBusy(true);
    setError(undefined);
    try {
      if (
        await openRepository(root, true, source === "empty" ? null : source)
      ) {
        onClose();
      }
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <DialogContent
        showCloseButton={false}
        className="border border-[var(--app-line)] bg-[var(--app-panel)] text-[var(--app-text)]"
      >
        <form
          className="space-y-3"
          onSubmit={(event) => {
            event.preventDefault();
            void create();
          }}
        >
          <DialogTitle className="text-sm">
            Set up repository collection
          </DialogTitle>
          <DialogDescription className="text-xs">
            Create <code>.conductor</code> in
            <span className="mt-1 block wrap-break-word" title={root}>
              {root}
            </span>
          </DialogDescription>
          <Select value={source} onValueChange={setSource} disabled={busy}>
            <SelectTrigger
              aria-label="Collection to share"
              className="w-full text-xs"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="empty">
                Start with an empty collection
              </SelectItem>
              {collections
                .filter((collection) => !collection.repository)
                .map((collection) => (
                  <SelectItem key={collection.id} value={collection.id}>
                    Copy {collection.name}
                  </SelectItem>
                ))}
            </SelectContent>
          </Select>
          {source !== "empty" ? (
            <p className="text-xs text-[var(--app-dim)]">
              Request definitions and enabled non-secret defaults will be
              shared. Private values go in .conductor/local.json. Review
              requests and variables for credentials before sharing.
            </p>
          ) : null}
          {error ? (
            <p
              role="alert"
              className="wrap-break-word text-xs text-destructive"
            >
              {error}
            </p>
          ) : null}
          <DialogFooter className="border-[var(--app-line)] bg-transparent">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={busy}
              onClick={onClose}
            >
              Cancel
            </Button>
            <Button type="submit" size="sm" disabled={busy}>
              {busy ? "Creating…" : "Create repository collection"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
