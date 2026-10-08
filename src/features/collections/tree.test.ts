import { describe, expect, it } from "vitest";
import type { CollectionNode } from "@/features/types";
import { moveNodeInTree, resolveDrop } from "./tree";

function node(
  id: string,
  position: number,
  parentId: string | null = null,
  children?: CollectionNode[],
): CollectionNode {
  const base = { id, parentId, position, name: id };
  return children
    ? { ...base, kind: "folder", children }
    : { ...base, kind: "request", requestId: `req-${id}`, method: "GET" };
}

function childrenOf(item: CollectionNode) {
  if (item.kind !== "folder") throw new Error(`${item.id} is not a folder`);
  return item.children;
}

// a, folder f (x, y), b
function sampleTree() {
  return [
    node("a", 0),
    node("f", 1, null, [node("x", 0, "f"), node("y", 1, "f")]),
    node("b", 2),
  ];
}

describe("moving a node", () => {
  it("closes the gap it left and numbers the new siblings from 0", () => {
    const tree = sampleTree();
    const moved = moveNodeInTree(tree, tree[0], "f", 1);

    expect(moved.map((item) => [item.id, item.position])).toEqual([
      ["f", 0],
      ["b", 1],
    ]);
    expect(
      childrenOf(moved[0]).map((item) => [
        item.id,
        item.position,
        item.parentId,
      ]),
    ).toEqual([
      ["x", 0, "f"],
      ["a", 1, "f"],
      ["y", 2, "f"],
    ]);
  });
});

describe("dropping a node", () => {
  it("lands after the target when dragging down within the same parent", () => {
    const tree = sampleTree();

    // `a` is removed first, so "after f" is index 1 among [f, b].
    expect(
      resolveDrop(tree, tree[0], {
        kind: "node",
        nodeId: "f",
        intent: "after",
      }),
    ).toEqual({ parentId: null, position: 1 });
  });

  it("keeps the target's index when dragging up", () => {
    const tree = sampleTree();

    expect(
      resolveDrop(tree, tree[2], {
        kind: "node",
        nodeId: "a",
        intent: "before",
      }),
    ).toEqual({ parentId: null, position: 0 });
  });

  it("does nothing when the drop would not change the order", () => {
    const tree = sampleTree();

    expect(
      resolveDrop(tree, tree[0], {
        kind: "node",
        nodeId: "f",
        intent: "before",
      }),
    ).toBeNull();
    expect(resolveDrop(tree, tree[2], { kind: "root-end" })).toBeNull();
  });

  it("appends to a folder, or to the root from inside one", () => {
    const tree = sampleTree();
    const x = childrenOf(tree[1])[0];

    expect(
      resolveDrop(tree, tree[0], {
        kind: "node",
        nodeId: "f",
        intent: "into",
      }),
    ).toEqual({ parentId: "f", position: 2 });
    expect(resolveDrop(tree, x, { kind: "root-end" })).toEqual({
      parentId: null,
      position: 3,
    });
  });
});
