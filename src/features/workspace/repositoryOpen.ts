import type { RepositoryPublishScope } from "@/bindings";
import { api } from "@/lib/tauri";
import { mayDiscardDrafts } from "./discardDrafts";
import {
  showCollection,
  useWorkspaceStore,
  waitForRepositoryRefresh,
} from "./workspaceStore";

export async function openRepository(
  root: string,
  create: boolean,
  source: string | null,
  preview?: RepositoryPublishScope[],
) {
  const get = useWorkspaceStore.getState;
  const approved = await mayDiscardDrafts(get, { copySource: source });
  if (!approved) return false;
  await waitForRepositoryRefresh();
  const opened = await api.openRepository(
    root,
    create,
    source,
    preview ?? null,
  );
  const switched = await showCollection(opened, approved);
  await get().loadEnvironments();
  // Creation can finish while a new draft keeps the old collection selected.
  return create || switched;
}
