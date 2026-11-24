import type { RequestDetail } from "@/features/types";
import { updateRequestNode } from "@/features/collections/tree";
import { api } from "@/lib/tauri";
import { getDraft, useDraftStore } from "./draftStore";
import {
  upsertTab,
  useWorkspaceStore,
  waitForRepositoryRefresh,
} from "./workspaceStore";

export async function saveDraft(
  request: RequestDetail,
  overwriteRevision?: string,
) {
  useWorkspaceStore.setState({ saving: true, error: undefined });
  try {
    if (request.repositoryRevision) await waitForRepositoryRefresh();
    const result = await api.saveRequest(
      overwriteRevision === undefined
        ? request
        : { ...request, repositoryRevision: overwriteRevision },
    );
    const draft = getDraft(request.id);
    if (result.status === "saved" && draft) {
      const current = result.revision
        ? { ...draft, repositoryRevision: result.revision }
        : draft;
      if (current !== draft) useDraftStore.getState().setDraft(current);
      useWorkspaceStore.setState((state) => ({
        tabs: upsertTab(state.tabs, current, draft !== request),
        tree: updateRequestNode(state.tree, request),
      }));
    }
    if (result.refresh) {
      const { applyRepositoryRefresh } = await import("./repositoryWorkspace");
      await applyRepositoryRefresh(result.refresh);
    }
    if (result.status === "failed") {
      const state = useWorkspaceStore.getState();
      if (state.activeRequestId === request.id) state.setError(result.message);
    } else if (result.status === "conflict") {
      if (result.revision === null)
        useDraftStore.getState().clearPreview(request.id);
      useWorkspaceStore.setState((state) => ({
        tabs: state.tabs.map((tab) =>
          tab.requestId === request.id
            ? { ...tab, conflict: { revision: result.revision } }
            : tab,
        ),
      }));
    } else if (draft) {
      await useWorkspaceStore.getState().resolveActiveRequest();
    }
  } catch (error) {
    const state = useWorkspaceStore.getState();
    if (state.activeRequestId === request.id) state.setError(error);
    if (request.repositoryRevision) await state.refreshRepositories();
  } finally {
    useWorkspaceStore.setState({ saving: false });
  }
}
