import { useCallback, useState } from "react";
import { api } from "@/lib/tauri";
import type { VariableChange } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

/**
 * The one save path for both variable editors: all changes go to Rust in one
 * transaction, a failure is shown in the app's error banner, and `saving`
 * always resets. Resolves to whether the save succeeded.
 */
export function useSaveVariables() {
  const [saving, setSaving] = useState(false);

  const save = useCallback(
    async (
      changes: VariableChange[],
      // Work that must succeed before the variables are written.
      prepare?: () => Promise<void>,
    ) => {
      const store = useWorkspaceStore.getState();
      setSaving(true);
      try {
        await prepare?.();
        if (changes.length) await api.applyVariableChanges(changes);
      } catch (error) {
        store.setError(error);
        return false;
      } finally {
        setSaving(false);
      }
      await store.resolveActiveRequest();
      return true;
    },
    [],
  );

  return { saving, save };
}
