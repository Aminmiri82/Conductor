import { confirm } from "@tauri-apps/plugin-dialog";
import { getDraft } from "./draftStore";
import type { RequestTab } from "./repositoryTabs";

type DraftWorkspace = { tabs: RequestTab[]; activeCollectionId?: string };

export async function mayDiscardDrafts(
  get: () => DraftWorkspace,
  { copySource }: { copySource?: string | null } = {},
) {
  const ask = (state: DraftWorkspace) =>
    confirm(
      copySource && copySource === state.activeCollectionId
        ? "Copy the saved collection and discard its unsaved request drafts?"
        : "Discard the unsaved request drafts in the current collection?",
      { kind: "warning" },
    );

  const before = get();
  const approved = new Map(
    before.tabs
      .filter((tab) => tab.dirty)
      .map((tab) => [tab.requestId, getDraft(tab.requestId)]),
  );
  if (approved.size && !(await ask(before))) return;

  // The collection command can take long enough for another unsaved edit.
  return () => {
    const current = get();
    const changed = current.tabs.some(
      (tab) =>
        tab.dirty &&
        (!approved.has(tab.requestId) ||
          approved.get(tab.requestId) !== getDraft(tab.requestId)),
    );
    return !changed || ask(current);
  };
}
