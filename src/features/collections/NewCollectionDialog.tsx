import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

export function NewCollectionDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const createCollection = useWorkspaceStore((state) => state.createCollection);
  const [name, setName] = useState("");

  function close() {
    setName("");
    onOpenChange(false);
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) close();
      }}
    >
      <DialogContent
        showCloseButton={false}
        aria-describedby={undefined}
        className="border border-[var(--app-line)] bg-[var(--app-panel)] text-[var(--app-text)] sm:max-w-sm"
      >
        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault();
            void createCollection(name);
            close();
          }}
        >
          <DialogTitle className="text-sm">New collection</DialogTitle>
          <Input
            autoFocus
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="Collection name"
            className="h-8 border-[var(--app-line)] bg-[var(--app-panel-2)] text-xs"
          />
          <DialogFooter className="border-[var(--app-line)] bg-transparent">
            <Button type="button" variant="ghost" size="sm" onClick={close}>
              Cancel
            </Button>
            <Button type="submit" size="sm">
              Create
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
