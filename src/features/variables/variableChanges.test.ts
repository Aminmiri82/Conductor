import { describe, expect, it } from "vite-plus/test";
import type { VariableEntry, VariableTarget } from "@/features/types";
import {
  applyRequestVariableChanges,
  diffVariables,
  duplicateKeys,
  requestVariableChanges,
  requestVariableRows,
  type RequestVariableRow,
  type ScopedVariables,
} from "./variableChanges";

const entry = (key: string, value: string): VariableEntry => ({
  key,
  value,
  enabled: true,
  sensitive: false,
});

describe("settings table changes", () => {
  it("flags keys that only differ by surrounding spaces as duplicates", () => {
    const rows = [entry("host", "1"), entry(" host ", "2"), entry("", "x")];

    expect([...duplicateKeys(rows)]).toEqual(["host"]);
  });

  it("sends only edited rows and the keys that disappeared", () => {
    const loaded = [entry("a", "1"), entry("b", "2"), entry("c", "3")];
    const current = [
      entry("a", "1"),
      { ...entry("b", "2"), sensitive: true },
      entry(" d ", "4"),
      entry("  ", "ignored"),
    ];

    expect(diffVariables(loaded, current)).toEqual({
      upserts: [{ ...entry("b", "2"), sensitive: true }, entry("d", "4")],
      deletes: ["c"],
    });
  });

  it("renaming a key deletes the old one and sets the new one", () => {
    expect(diffVariables([entry("old", "v")], [entry("new", "v")])).toEqual({
      upserts: [entry("new", "v")],
      deletes: ["old"],
    });
  });

  it("sends a repeated key every time so Rust can reject it", () => {
    const { upserts } = diffVariables(
      [entry("a", "1")],
      [entry("a", "1"), entry("a", "1")],
    );
    expect(upserts).toHaveLength(2);
  });
});

describe("request variables tab changes", () => {
  const targetFor = (scope: VariableTarget["scope"]): VariableTarget =>
    scope === "global"
      ? { scope }
      : scope === "collection"
        ? { scope, collectionId: "col" }
        : { scope, environmentId: "env" };
  const row = (patch: Partial<RequestVariableRow>): RequestVariableRow => ({
    key: "k",
    value: "v",
    scope: "collection",
    originalScope: "collection",
    enabled: true,
    sensitive: false,
    ...patch,
  });
  const nothingStored: ScopedVariables = {
    global: [],
    collection: [],
    environment: [],
  };

  it("shows the environment's value over the collection's and the global one", () => {
    const rows = requestVariableRows(
      ["a", "b", "c"],
      {
        global: [entry("a", "global"), entry("b", "global")],
        collection: [entry("a", "collection")],
        environment: [entry("a", "environment")],
      },
      true,
    );

    expect(
      rows.map(({ key, value, originalScope }) => [key, value, originalScope]),
    ).toEqual([
      ["a", "environment", "environment"],
      ["b", "global", "global"],
      ["c", "", null],
    ]);
  });

  it("offers a new variable to the environment, or to the collection when none is active", () => {
    expect(requestVariableRows(["k"], nothingStored, true)[0].scope).toBe(
      "environment",
    );
    expect(requestVariableRows(["k"], nothingStored, false)[0].scope).toBe(
      "collection",
    );
  });

  it("moving a row deletes it from its old scope and sets it in the new one", () => {
    const changes = requestVariableChanges({
      edited: [row({ scope: "environment" })],
      removed: [],
      targetFor,
    });

    expect(changes).toEqual([
      {
        target: targetFor("collection"),
        upserts: [],
        deletes: ["k"],
      },
      {
        target: targetFor("environment"),
        upserts: [entry("k", "v")],
        deletes: [],
      },
    ]);
  });

  it("creates a variable that does not exist yet once its row is edited", () => {
    const changes = requestVariableChanges({
      edited: [row({ key: "fresh", originalScope: null, value: "new" })],
      removed: [{ scope: "global", key: "gone" }],
      targetFor,
    });

    expect(changes).toEqual([
      { target: targetFor("global"), upserts: [], deletes: ["gone"] },
      {
        target: targetFor("collection"),
        upserts: [entry("fresh", "new")],
        deletes: [],
      },
    ]);
  });

  it("after removing a variable, the row falls back to the next scope's value", () => {
    const stored = applyRequestVariableChanges(
      {
        global: [entry("k", "global")],
        collection: [],
        environment: [entry("k", "environment")],
      },
      [],
      [{ scope: "environment", key: "k" }],
    );

    expect(requestVariableRows(["k"], stored, true)[0]).toMatchObject({
      value: "global",
      originalScope: "global",
    });
  });

  it("keeps a value typed into a row whose removal is still pending", () => {
    // Deletes apply first, in Rust and here, so the typed value wins.
    const stored = applyRequestVariableChanges(
      { ...nothingStored, collection: [entry("k", "old")] },
      [row({ value: "typed", originalScope: null })],
      [{ scope: "collection", key: "k" }],
    );

    expect(stored.collection).toEqual([entry("k", "typed")]);
  });

  it("a saved move leaves the variable only in its new scope", () => {
    const stored = applyRequestVariableChanges(
      { ...nothingStored, collection: [entry("k", "v"), entry("other", "x")] },
      [row({ scope: "global" })],
      [],
    );

    expect(stored).toEqual({
      global: [entry("k", "v")],
      collection: [entry("other", "x")],
      environment: [],
    });
  });
});
