import { createContext, useContext, useMemo, useState } from "react";
import type { CollectionNode } from "@/features/types";
import { collectNodeIds } from "@/features/workspace/collectionTree";

export type DropIntent = "before" | "after" | "into";
export type DropTarget =
  | { kind: "node"; nodeId: string; intent: DropIntent }
  | { kind: "root-end" }
  | null;

export type DnDValue = {
  draggingId: string | null;
  draggingNode: CollectionNode | null;
  draggingDescendantIds: Set<string>;
  beginDrag: (node: CollectionNode) => void;
  endDrag: () => void;
  target: DropTarget;
  setTarget: (target: DropTarget) => void;
};

export const DnDContext = createContext<DnDValue | null>(null);

export function useDnD(): DnDValue {
  const value = useContext(DnDContext);
  if (!value) throw new Error("DnD context missing");
  return value;
}

export function useTreeDnDValue(): DnDValue {
  const [draggingNode, setDraggingNode] = useState<CollectionNode | null>(null);
  const [draggingDescendantIds, setDraggingDescendantIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [target, setTarget] = useState<DropTarget>(null);

  return useMemo<DnDValue>(
    () => ({
      draggingId: draggingNode?.id ?? null,
      draggingNode,
      draggingDescendantIds,
      beginDrag: (node) => {
        setDraggingNode(node);
        setDraggingDescendantIds(collectNodeIds(node));
        setTarget(null);
      },
      endDrag: () => {
        setDraggingNode(null);
        setDraggingDescendantIds(new Set());
        setTarget(null);
      },
      target,
      setTarget,
    }),
    [draggingDescendantIds, draggingNode, target],
  );
}
