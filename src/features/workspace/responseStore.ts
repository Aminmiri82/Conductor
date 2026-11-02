import { create } from "zustand";
import { api } from "@/lib/tauri";
import type { SendRequestResult } from "@/features/types";

// Each open tab's latest response, without its body: Rust keeps bodies on
// disk and the viewer reads only the one it shows. Removing a response here
// discards its body in Rust.
type ResponseState = {
  responses: Map<string, SendRequestResult>;
  revision: number;
  setResponse: (requestId: string, response: SendRequestResult) => void;
  removeMany: (requestIds: string[]) => void;
  clearAll: () => void;
};

export const useResponseStore = create<ResponseState>((set, get) => ({
  responses: new Map(),
  revision: 0,

  setResponse: (requestId, response) => {
    set((state) => {
      state.responses.set(requestId, response);
      return { revision: state.revision + 1 };
    });
  },

  removeMany: (requestIds) => {
    const removed = requestIds.filter((id) => get().responses.has(id));
    if (!removed.length) return;
    void api.discardResponses(removed);
    set((state) => {
      for (const requestId of removed) {
        state.responses.delete(requestId);
      }
      return { revision: state.revision + 1 };
    });
  },

  clearAll: () => get().removeMany([...get().responses.keys()]),
}));
