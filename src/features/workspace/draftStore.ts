import { create } from "zustand";
import type { EntityId, RequestDetail, ResolvedRequestPreview } from "@/features/types";

type DraftState = {
  drafts: Map<EntityId, RequestDetail>;
  dirtyRequestIds: Set<EntityId>;
  previews: Map<EntityId, ResolvedRequestPreview>;
  lastSavedAtById: Map<EntityId, number>;
  revision: number;
  setDraft: (request: RequestDetail, dirty?: boolean) => void;
  updateDraft: (requestId: EntityId, updater: (draft: RequestDetail) => RequestDetail) => RequestDetail | undefined;
  markSaved: (requestId: EntityId) => void;
  setPreview: (requestId: EntityId, preview: ResolvedRequestPreview) => void;
  removeMany: (requestIds: EntityId[]) => void;
  clearAll: () => void;
};

export const useDraftStore = create<DraftState>((set, get) => ({
  drafts: new Map(),
  dirtyRequestIds: new Set(),
  previews: new Map(),
  lastSavedAtById: new Map(),
  revision: 0,

  setDraft: (request, dirty = false) => {
    set((state) => {
      state.drafts.set(request.id, request);
      if (dirty) state.dirtyRequestIds.add(request.id);
      else state.dirtyRequestIds.delete(request.id);
      return { revision: state.revision + 1 };
    });
  },

  updateDraft: (requestId, updater) => {
    const current = get().drafts.get(requestId);
    if (!current) return undefined;
    const next = updater(current);
    set((state) => {
      state.drafts.set(requestId, next);
      state.dirtyRequestIds.add(requestId);
      state.lastSavedAtById.delete(requestId);
      return { revision: state.revision + 1 };
    });
    return next;
  },

  markSaved: (requestId) => {
    set((state) => {
      state.dirtyRequestIds.delete(requestId);
      state.lastSavedAtById.set(requestId, Date.now());
      return { revision: state.revision + 1 };
    });
  },

  setPreview: (requestId, preview) => {
    set((state) => {
      state.previews.set(requestId, preview);
      return { revision: state.revision + 1 };
    });
  },

  removeMany: (requestIds) => {
    if (!requestIds.length) return;
    set((state) => {
      for (const requestId of requestIds) {
        state.drafts.delete(requestId);
        state.dirtyRequestIds.delete(requestId);
        state.previews.delete(requestId);
        state.lastSavedAtById.delete(requestId);
      }
      return { revision: state.revision + 1 };
    });
  },

  clearAll: () => {
    set((state) => {
      state.drafts.clear();
      state.dirtyRequestIds.clear();
      state.previews.clear();
      state.lastSavedAtById.clear();
      return { revision: state.revision + 1 };
    });
  },
}));

export function getDraft(requestId: EntityId | undefined): RequestDetail | undefined {
  return requestId ? useDraftStore.getState().drafts.get(requestId) : undefined;
}

export function getPreview(requestId: EntityId | undefined): ResolvedRequestPreview | undefined {
  return requestId ? useDraftStore.getState().previews.get(requestId) : undefined;
}

export function isDraftDirty(requestId: EntityId | undefined): boolean {
  return requestId ? useDraftStore.getState().dirtyRequestIds.has(requestId) : false;
}
