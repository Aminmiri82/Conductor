import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import type { VariableTarget } from "@/features/types";
import { api } from "@/lib/tauri";

export function SharedVariableDialog({
  target,
  variableKey,
  onClose,
  prepare,
}: {
  target: VariableTarget;
  variableKey?: string;
  onClose: () => void;
  prepare?: () => Promise<void>;
}) {
  const declaring = variableKey === undefined;
  const title = declaring ? "Declare required secret" : "Edit shared value";
  const [previous, setPrevious] = useState<string>();
  const [text, setText] = useState("");
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (variableKey === undefined) return;
    let open = true;
    api.getSharedVariable(target, variableKey).then(
      (value) => {
        if (!open) return;
        setPrevious(value);
        setText(value);
      },
      (error: unknown) => {
        if (open) setError(String(error));
      },
    );
    return () => {
      open = false;
    };
  }, [target, variableKey]);

  const ready = declaring ? Boolean(text.trim()) : previous !== undefined;
  async function save() {
    const change =
      variableKey === undefined
        ? ({ kind: "requiredSecret" } as const)
        : previous === undefined
          ? undefined
          : ({ kind: "value", previous, value: text } as const);
    if (!change) return;
    setSaving(true);
    setError(undefined);
    try {
      await prepare?.();
      const refresh = await api.saveSharedVariable(
        target,
        variableKey ?? text,
        change,
      );
      const { applyRepositoryRefresh } =
        await import("@/features/workspace/repositoryWorkspace");
      await applyRepositoryRefresh(refresh);
      onClose();
    } catch (error) {
      setError(String(error));
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog open onOpenChange={(open) => !open && !saving && onClose()}>
      <DialogContent showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>
            {declaring
              ? "The repository stores the name. Each teammate sets their value locally."
              : `Change the shared default for ${variableKey} in the repository.`}
          </DialogDescription>
        </DialogHeader>
        <form
          className="grid gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (ready && !saving) void save();
          }}
        >
          <label className="grid gap-1.5 text-xs">
            {declaring ? "Secret key" : "Shared value"}
            <Input
              value={text}
              disabled={saving}
              readOnly={!declaring && previous === undefined}
              onChange={(event) => setText(event.target.value)}
            />
          </label>
          {error ? (
            <p role="alert" className="text-xs text-destructive">
              {error}
            </p>
          ) : null}
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={onClose}
              disabled={saving}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={!ready || saving}>
              {declaring ? "Declare" : "Save shared value"}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
