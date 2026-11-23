import { describe, expect, it } from "vite-plus/test";
import { reconcileRepositoryTabs } from "./repositoryTabs";

const draft = {
  requestId: "edited",
  name: "Draft",
  method: "POST",
  dirty: true,
};
const clean = {
  requestId: "clean",
  name: "Saved",
  method: "GET",
  dirty: false,
};

describe("repository tab refresh", () => {
  it("keeps a dirty draft when its file changes or is removed", () => {
    const changed = reconcileRepositoryTabs(
      [draft],
      [{ requestId: draft.requestId, revision: "other" }],
      () => "original",
    );
    expect(changed.tabs).toEqual([
      { ...draft, conflict: { revision: "other" } },
    ]);
    expect(changed.invalidate).toEqual([]);
    const removed = reconcileRepositoryTabs(
      changed.tabs,
      [{ requestId: draft.requestId, revision: null }],
      () => "original",
    );
    expect(removed.tabs).toEqual([{ ...draft, conflict: { revision: null } }]);
    expect(removed.invalidate).toEqual([]);
  });

  it("invalidates a clean draft and closes its tab only when the file is gone", () => {
    const changed = reconcileRepositoryTabs(
      [clean, draft],
      [{ requestId: clean.requestId, revision: "other" }],
      () => "original",
    );
    expect(changed.tabs).toEqual([clean, draft]);
    expect(changed.invalidate).toEqual([clean.requestId]);
    const removed = reconcileRepositoryTabs(
      changed.tabs,
      [{ requestId: clean.requestId, revision: null }],
      () => "original",
    );
    expect(removed.tabs).toEqual([draft]);
    expect(removed.invalidate).toEqual([clean.requestId]);
  });

  it("leaves a draft alone when it already has the observed revision", () => {
    const tabs = [draft];
    const result = reconcileRepositoryTabs(
      tabs,
      [{ requestId: draft.requestId, revision: "saved" }],
      () => "saved",
    );
    expect(result.tabs).toBe(tabs);
    expect(result.invalidate).toEqual([]);
  });
});
