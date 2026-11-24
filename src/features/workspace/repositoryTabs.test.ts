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
      [
        {
          requestId: draft.requestId,
          revision: "other",
          name: draft.name,
          method: draft.method,
        },
      ],
      () => "original",
    );
    expect(changed.tabs).toEqual([
      { ...draft, conflict: { revision: "other" } },
    ]);
    expect(changed.invalidate).toEqual([]);
    const removed = reconcileRepositoryTabs(
      changed.tabs,
      [
        {
          requestId: draft.requestId,
          revision: null,
          name: null,
          method: null,
        },
      ],
      () => "original",
    );
    expect(removed.tabs).toEqual([{ ...draft, conflict: { revision: null } }]);
    expect(removed.closed).toEqual([]);
    expect(removed.invalidate).toEqual([]);
  });

  it("invalidates a clean draft and closes its tab only when the file is gone", () => {
    const changed = reconcileRepositoryTabs(
      [clean, draft],
      [
        {
          requestId: clean.requestId,
          revision: "other",
          name: clean.name,
          method: clean.method,
        },
      ],
      () => "original",
    );
    expect(changed.tabs).toEqual([clean, draft]);
    expect(changed.invalidate).toEqual([clean.requestId]);
    expect(changed.closed).toEqual([]);
    const removed = reconcileRepositoryTabs(
      changed.tabs,
      [
        {
          requestId: clean.requestId,
          revision: null,
          name: null,
          method: null,
        },
      ],
      () => "original",
    );
    expect(removed.tabs).toEqual([draft]);
    expect(removed.invalidate).toEqual([clean.requestId]);
    expect(removed.closed).toEqual([clean.requestId]);
  });

  it("updates a clean tab's label and method from the refreshed request", () => {
    const result = reconcileRepositoryTabs(
      [draft, clean],
      [
        {
          requestId: draft.requestId,
          revision: "new",
          name: "Ignored",
          method: "GET",
        },
        {
          requestId: clean.requestId,
          revision: "new",
          name: "Renamed",
          method: "PATCH",
        },
      ],
      () => "old",
    );
    expect(result.tabs).toEqual([
      { ...draft, conflict: { revision: "new" } },
      { ...clean, name: "Renamed", method: "PATCH" },
    ]);
    expect(result.invalidate).toEqual([clean.requestId]);
  });

  it("allows a dirty draft to send again when its original request returns", () => {
    const result = reconcileRepositoryTabs(
      [{ ...draft, conflict: { revision: null } }],
      [
        {
          requestId: draft.requestId,
          revision: "original",
          name: draft.name,
          method: draft.method,
        },
      ],
      () => "original",
    );

    expect(result.tabs).toEqual([draft]);
    expect(result.invalidate).toEqual([]);
  });

  it("leaves a draft alone when it already has the observed revision", () => {
    const tabs = [draft];
    const result = reconcileRepositoryTabs(
      tabs,
      [
        {
          requestId: draft.requestId,
          revision: "saved",
          name: draft.name,
          method: draft.method,
        },
      ],
      () => "saved",
    );
    expect(result.tabs).toBe(tabs);
    expect(result.invalidate).toEqual([]);
  });
});
