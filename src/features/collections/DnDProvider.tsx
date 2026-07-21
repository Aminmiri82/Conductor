import { type ReactNode } from "react";
import { DnDContext, type DnDValue } from "@/features/collections/treeDnD";

export function DnDProvider({
  children,
  value,
}: {
  children: ReactNode;
  value: DnDValue;
}) {
  return <DnDContext.Provider value={value}>{children}</DnDContext.Provider>;
}
