import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { resolveRepositoryConflict } from "@/features/workspace/repositoryWorkspace";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

export function RepositoryConflict({ requestId }: { requestId: string }) {
  const conflict = useWorkspaceStore(
    (state) => state.tabs.find((tab) => tab.requestId === requestId)?.conflict,
  );
  const saving = useWorkspaceStore((state) => state.saving);
  if (!conflict) return null;
  const removed = conflict.revision === null;
  return (
    <div
      role="status"
      className="flex flex-wrap items-center justify-between gap-2 border-b border-amber-400/20 bg-amber-400/10 px-4 py-2 text-xs text-amber-200"
    >
      <span className="flex items-center gap-2">
        <AlertTriangle aria-hidden="true" className="size-3.5 shrink-0" />
        {removed
          ? "This request was removed from the repository."
          : "This request changed in the repository."}
        {" Your unsaved draft is still open."}
      </span>
      <div className="flex items-center gap-1">
        <Button
          variant="ghost"
          size="sm"
          disabled={saving}
          onClick={() => void resolveRepositoryConflict("reload")}
        >
          {removed ? "Discard draft" : "Reload repository version"}
        </Button>
        <Button
          variant="outline"
          size="sm"
          disabled={saving}
          onClick={() =>
            void resolveRepositoryConflict(removed ? "save-as-new" : "keep")
          }
        >
          {removed ? "Save as new request" : "Overwrite with my draft"}
        </Button>
      </div>
    </div>
  );
}
