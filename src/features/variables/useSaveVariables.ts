import { useCallback, useState } from "react";
import { api } from "@/lib/tauri";
import type { VariableChange } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

/**
 * The one save path for both variable editors: all changes go to Rust in one
 * transaction, a failure is shown in the app's error banner, and `saving`
 * always resets. On success reports whether linked rows need to be reloaded.
 */
export function useSaveVariables() {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string>();

  const save = useCallback(
    async (
      changes: VariableChange[],
      // Work that must succeed before the variables are written.
      prepare?: () => Promise<void>,
    ) => {
      const store = useWorkspaceStore.getState();
      setSaving(true);
      setError(undefined);
      let needsReload = false;
      try {
        await prepare?.();
        if (changes.length)
          needsReload = await api.applyVariableChanges(changes);
      } catch (error) {
        setError(String(error));
        store.setError(error);
        return false;
      } finally {
        setSaving(false);
      }
      await store.resolveActiveRequest();
      return { needsReload };
    },
    [],
  );

  return { saving, save, error };
}
