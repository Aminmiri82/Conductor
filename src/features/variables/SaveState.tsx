export type SaveStatus = "idle" | "saving" | "saved" | "failed";

/** Where an autosaving variable table says whether its edits have landed. */
export function SaveState({
  status,
  duplicate,
  onRetry,
}: {
  status: SaveStatus;
  // A key used twice, which blocks the save until it is fixed.
  duplicate?: string;
  onRetry: () => void;
}) {
  if (duplicate) {
    return (
      <span className="text-xs text-destructive">
        Not saved: “{duplicate}” is used twice
      </span>
    );
  }
  if (status === "failed") {
    return (
      <button className="text-xs text-destructive underline" onClick={onRetry}>
        Not saved. Retry
      </button>
    );
  }
  return (
    <span className="text-xs text-[var(--app-dim)]" aria-live="polite">
      {status === "saving" ? "Saving…" : status === "saved" ? "Saved" : ""}
    </span>
  );
}
