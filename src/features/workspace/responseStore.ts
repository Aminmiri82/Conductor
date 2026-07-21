import { create } from "zustand";
import type { EntityId, SendRequestResult } from "@/features/types";

type ResponseState = {
  responses: Map<EntityId, SendRequestResult>;
  revision: number;
  setResponse: (requestId: EntityId, response: SendRequestResult) => void;
  removeMany: (requestIds: EntityId[]) => void;
  clearAll: () => void;
};

export const useResponseStore = create<ResponseState>((set) => ({
  responses: new Map(),
  revision: 0,

  setResponse: (requestId, response) => {
    set((state) => {
      state.responses.set(requestId, response);
      return { revision: state.revision + 1 };
    });
  },

  removeMany: (requestIds) => {
    if (!requestIds.length) return;
    set((state) => {
      for (const requestId of requestIds) {
        state.responses.delete(requestId);
      }
      return { revision: state.revision + 1 };
    });
  },

  clearAll: () => {
    set((state) => {
      state.responses.clear();
      return { revision: state.revision + 1 };
    });
  },
}));

export function getResponse(requestId: EntityId | undefined): SendRequestResult | undefined {
  return requestId ? useResponseStore.getState().responses.get(requestId) : undefined;
}
