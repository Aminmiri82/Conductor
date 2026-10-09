import { describe, expect, it } from "vitest";
import type { VariableEntry, VariableTarget } from "@/features/types";
import {
  diffVariables,
  duplicateKeys,
  requestVariableChanges,
  type RequestVariableRow,
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

  it("moving a row deletes it from its old scope and sets it in the new one", () => {
    const changes = requestVariableChanges({
      rows: [row({ scope: "environment" })],
      edited: new Set(["k"]),
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

  it("leaves untouched stored rows out but creates variables that do not exist yet", () => {
    const changes = requestVariableChanges({
      rows: [
        row({ key: "stored" }),
        row({ key: "fresh", originalScope: null, value: "" }),
      ],
      edited: new Set(),
      removed: [{ scope: "global", key: "gone" }],
      targetFor,
    });

    expect(changes).toEqual([
      { target: targetFor("global"), upserts: [], deletes: ["gone"] },
      {
        target: targetFor("collection"),
        upserts: [entry("fresh", "")],
        deletes: [],
      },
    ]);
  });
});
