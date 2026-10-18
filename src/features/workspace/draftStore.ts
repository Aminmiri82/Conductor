import { create } from "zustand";
import type { RequestDetail, ResolvedRequestPreview } from "@/features/types";

type DraftState = {
  drafts: Map<string, RequestDetail>;
  previews: Map<string, ResolvedRequestPreview>;
  revision: number;
  setDraft: (request: RequestDetail) => void;
  setPreview: (requestId: string, preview: ResolvedRequestPreview) => void;
  removeMany: (requestIds: string[]) => void;
  clearAll: () => void;
};

export const useDraftStore = create<DraftState>((set) => ({
  drafts: new Map(),
  previews: new Map(),
  revision: 0,

  setDraft: (request) => {
    set((state) => {
      state.drafts.set(request.id, request);
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
        state.previews.delete(requestId);
      }
      return { revision: state.revision + 1 };
    });
  },

  clearAll: () => {
    set((state) => {
      state.drafts.clear();
      state.previews.clear();
      return { revision: state.revision + 1 };
    });
  },
}));

export function getDraft(
  requestId: string | undefined,
): RequestDetail | undefined {
  return requestId ? useDraftStore.getState().drafts.get(requestId) : undefined;
}

// Subscribes to one field of a draft, so a component re-renders only when
// that field changes.
export function useDraftValue<T>(
  requestId: string,
  select: (request: RequestDetail) => T,
): T | undefined {
  return useDraftStore((state) => {
    const draft = state.drafts.get(requestId);
    return draft && select(draft);
  });
}
