import { useState } from "react";
import type { RepositoryPublishScope } from "@/bindings";
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
import { api } from "@/lib/tauri";

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
  const [operation, setOperation] = useState<"preview" | "create">();
  const [preview, setPreview] = useState<RepositoryPublishScope[]>();
  const busy = operation !== undefined;
  const [error, setError] = useState<string>();

  async function review(collectionId: string) {
    setSource(collectionId);
    setPreview(undefined);
    setError(undefined);
    if (collectionId === "empty") return;
    setOperation("preview");
    try {
      setPreview(await api.previewRepositoryPublish(collectionId));
    } catch (error) {
      setError(String(error));
    } finally {
      setOperation(undefined);
    }
  }

  function requireSecret(environmentId: string | null, key: string) {
    if (!preview) return;
    setPreview(
      preview.map((scope) =>
        scope.environmentId === environmentId
          ? {
              ...scope,
              sharedKeys: scope.sharedKeys.filter((shared) => shared !== key),
              secretKeys: [...scope.secretKeys, key],
            }
          : scope,
      ),
    );
  }

  async function create() {
    if (source !== "empty" && !preview) return;
    setOperation("create");
    setError(undefined);
    try {
      if (
        await openRepository(
          root,
          true,
          source === "empty" ? null : source,
          preview,
        )
      ) {
        onClose();
      }
    } catch (error) {
      setError(String(error));
    } finally {
      setOperation(undefined);
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
          <Select
            value={source}
            onValueChange={(value) => void review(value)}
            disabled={busy}
          >
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
          {source !== "empty" && (
            <p className="text-xs text-[var(--app-dim)]">
              Request definitions and shared values go in Git. Required secrets
              keep their values in <code>.conductor/local.json</code>. Review
              free-form fields and custom headers before committing.
            </p>
          )}
          {operation === "preview" && (
            <p role="status" className="text-xs text-[var(--app-dim)]">
              Reviewing variables…
            </p>
          )}
          {preview && (
            <div className="max-h-60 space-y-3 overflow-y-auto rounded-md border border-[var(--app-line)] p-2 text-xs">
              {preview.map((scope) => (
                <section key={scope.environmentId ?? "collection"}>
                  <h3 className="mb-1 font-medium">
                    {scope.environmentId === null ? "Collection" : scope.name}
                  </h3>
                  {!scope.sharedKeys.length && !scope.secretKeys.length ? (
                    <p className="text-[var(--app-dim)]">
                      No shared values or required secrets
                    </p>
                  ) : (
                    <ul className="space-y-1">
                      {[
                        ...scope.sharedKeys.map((key) => ({
                          key,
                          shared: true,
                        })),
                        ...scope.secretKeys.map((key) => ({
                          key,
                          shared: false,
                        })),
                      ].map(({ key, shared }) => (
                        <li
                          key={key}
                          className="flex min-h-6 items-center gap-2"
                        >
                          <code className="min-w-0 flex-1 wrap-break-word">
                            {key}
                          </code>
                          <span className="text-[var(--app-dim)]">
                            {shared ? "Shared value" : "Required secret"}
                          </span>
                          {shared && (
                            <Button
                              type="button"
                              variant="ghost"
                              size="sm"
                              className="h-6 px-1.5 text-xs"
                              aria-label={`Require secret for ${key}`}
                              disabled={busy}
                              onClick={() =>
                                requireSecret(scope.environmentId, key)
                              }
                            >
                              Require secret
                            </Button>
                          )}
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
              ))}
            </div>
          )}
          {error ? (
            <p
              role="alert"
              className="wrap-break-word text-xs text-destructive"
            >
              {error}
            </p>
          ) : null}
          <DialogFooter className="border-[var(--app-line)] bg-transparent">
            {error && source !== "empty" && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={busy}
                onClick={() => void review(source)}
              >
                Refresh preview
              </Button>
            )}
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={busy}
              onClick={onClose}
            >
              Cancel
            </Button>
            <Button
              type="submit"
              size="sm"
              disabled={busy || (source !== "empty" && !preview)}
            >
              {operation === "create"
                ? "Creating…"
                : "Create repository collection"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
