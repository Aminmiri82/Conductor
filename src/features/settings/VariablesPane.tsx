import { type ChangeEvent, useEffect, useRef, useState } from "react";
import { Eye, EyeOff, Plus, Save, Trash2, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { api } from "@/lib/tauri";
import type { VariableEntry } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";
import { SectionTitle, Segmented } from "@/features/settings/settingsShared";

type Scope = "environment" | "collection" | "global";

export function VariablesPane({
  activeCollectionId,
}: {
  activeCollectionId?: string;
}) {
  const collections = useWorkspaceStore((state) => state.collections);
  const environments = useWorkspaceStore((state) => state.environments);
  const activeEnvironmentId = useWorkspaceUiStore(
    (state) => state.workspaceUi.activeEnvironmentId,
  );
  const selectEnvironment = useWorkspaceStore(
    (state) => state.selectEnvironment,
  );
  const loadEnvironments = useWorkspaceStore((state) => state.loadEnvironments);
  const importEnvironment = useWorkspaceStore(
    (state) => state.importEnvironment,
  );
  const [scope, setScope] = useState<Scope>(
    activeCollectionId || collections[0] ? "collection" : "global",
  );
  const [selectedCollectionId, setSelectedCollectionId] = useState(
    activeCollectionId ?? collections[0]?.id ?? "",
  );
  const [selectedEnvironmentId, setSelectedEnvironmentId] = useState(
    activeEnvironmentId ?? environments[0]?.id ?? "",
  );
  const [variables, setVariables] = useState<VariableEntry[]>([]);
  const [saving, setSaving] = useState(false);
  const [environmentName, setEnvironmentName] = useState("");
  const [revealed, setRevealed] = useState<Record<number, boolean>>({});
  const environmentInputRef = useRef<HTMLInputElement>(null);
  const resolveActiveRequest = useWorkspaceStore(
    (state) => state.resolveActiveRequest,
  );

  const collectionId = scope === "collection" ? selectedCollectionId : null;
  const environmentId = scope === "environment" ? selectedEnvironmentId : null;
  const canEditVariables =
    scope === "global" || Boolean(collectionId) || Boolean(environmentId);

  useEffect(() => {
    if (selectedCollectionId || !collections[0]) return;
    setSelectedCollectionId(collections[0].id);
  }, [collections, selectedCollectionId]);

  useEffect(() => {
    if (activeEnvironmentId) {
      setSelectedEnvironmentId(activeEnvironmentId);
      return;
    }
    if (selectedEnvironmentId || !environments[0]) return;
    setSelectedEnvironmentId(environments[0].id);
  }, [activeEnvironmentId, environments, selectedEnvironmentId]);

  useEffect(() => {
    const environment = environments.find(
      (environment) => environment.id === selectedEnvironmentId,
    );
    setEnvironmentName(environment?.name ?? "");
  }, [environments, selectedEnvironmentId]);

  useEffect(() => {
    let mounted = true;
    if (!canEditVariables) {
      setVariables([]);
      return;
    }
    void api.listVariables(scope, collectionId, environmentId).then((items) => {
      if (mounted) setVariables(items);
    });
    return () => {
      mounted = false;
    };
  }, [scope, collectionId, environmentId, canEditVariables]);

  async function save() {
    if (!canEditVariables) return;
    setSaving(true);
    const currentEnvironmentName = environments.find(
      (environment) => environment.id === selectedEnvironmentId,
    )?.name;
    if (
      scope === "environment" &&
      selectedEnvironmentId &&
      environmentName.trim() &&
      environmentName.trim() !== currentEnvironmentName
    ) {
      await api.renameEnvironment(selectedEnvironmentId, environmentName);
      await loadEnvironments();
    }
    await api.saveVariables(
      scope,
      collectionId,
      environmentId,
      variables.map((variable) => ({
        ...variable,
        scope,
        collectionId,
        environmentId,
      })),
    );
    setSaving(false);
    await resolveActiveRequest();
  }

  async function createEnvironment() {
    const environmentId = await api.createEnvironment("New environment");
    await loadEnvironments();
    setScope("environment");
    setSelectedEnvironmentId(environmentId);
    await selectEnvironment(environmentId);
  }

  async function onEnvironmentFileChange(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    await importEnvironment(text, file.name);
    setScope("environment");
    event.target.value = "";
  }

  async function deleteSelectedEnvironment() {
    if (!selectedEnvironmentId) return;
    const environment = environments.find(
      (environment) => environment.id === selectedEnvironmentId,
    );
    const confirmed = window.confirm(
      `Delete environment "${environment?.name ?? "selected environment"}"?`,
    );
    if (!confirmed) return;

    const nextEnvironmentId =
      environments.find(
        (environment) => environment.id !== selectedEnvironmentId,
      )?.id ?? null;
    await api.deleteEnvironment(selectedEnvironmentId);
    await loadEnvironments();
    setSelectedEnvironmentId(nextEnvironmentId ?? "");
    await selectEnvironment(nextEnvironmentId);
  }

  function updateVariable(index: number, patch: Partial<VariableEntry>) {
    setVariables((current) =>
      current.map((variable, i) =>
        i === index ? { ...variable, ...patch } : variable,
      ),
    );
  }

  function removeVariable(index: number) {
    setVariables((current) => current.filter((_, i) => i !== index));
    setRevealed((current) =>
      Object.fromEntries(
        Object.entries(current)
          .filter(([key]) => Number(key) !== index)
          .map(([key, value]) => [
            Number(key) > index ? String(Number(key) - 1) : key,
            value,
          ]),
      ),
    );
  }

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-start gap-3">
        <SectionTitle
          title="Variables"
          sub="Environment variables override collection and global variables."
        />
        <div className="flex-1" />
        <Segmented
          value={scope}
          options={[
            ["environment", "Environment"],
            ["collection", "Collection"],
            ["global", "Global"],
          ]}
          onChange={(value) => setScope(value as Scope)}
        />
        {scope === "environment" ? (
          <>
            <Select
              value={selectedEnvironmentId}
              onValueChange={(environmentId) => {
                setSelectedEnvironmentId(environmentId);
                void selectEnvironment(environmentId);
              }}
              disabled={!environments.length}
            >
              <SelectTrigger className="h-8 w-56 border-[var(--app-line)] bg-[var(--app-panel-2)] text-xs">
                <SelectValue placeholder="Select environment" />
              </SelectTrigger>
              <SelectContent>
                {environments.map((environment) => (
                  <SelectItem key={environment.id} value={environment.id}>
                    {environment.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <input
              ref={environmentInputRef}
              type="file"
              accept="application/json,.json,.env,text/plain"
              className="hidden"
              onChange={(event) => void onEnvironmentFileChange(event)}
            />
            <Input
              className="h-8 w-48 border-[var(--app-line)] bg-[var(--app-panel-2)] text-xs"
              value={environmentName}
              onChange={(event) => setEnvironmentName(event.target.value)}
              disabled={!selectedEnvironmentId}
              placeholder="Environment name"
            />
            <Button
              variant="ghost"
              className="h-8 gap-1.5 px-2.5 text-xs"
              onClick={createEnvironment}
            >
              <Plus className="size-3.5" />
              New env
            </Button>
            <Button
              variant="ghost"
              className="h-8 gap-1.5 px-2.5 text-xs"
              onClick={() => environmentInputRef.current?.click()}
            >
              <Upload className="size-3.5" />
              Import env
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="size-8 text-[var(--app-dim)] hover:text-destructive"
              onClick={() => void deleteSelectedEnvironment()}
              disabled={!selectedEnvironmentId}
              title="Delete environment"
            >
              <Trash2 className="size-3.5" />
            </Button>
          </>
        ) : null}
        {scope === "collection" ? (
          <Select
            value={selectedCollectionId}
            onValueChange={setSelectedCollectionId}
            disabled={!collections.length}
          >
            <SelectTrigger className="h-8 w-56 border-[var(--app-line)] bg-[var(--app-panel-2)] text-xs">
              <SelectValue placeholder="Select collection" />
            </SelectTrigger>
            <SelectContent>
              {collections.map((collection) => (
                <SelectItem key={collection.id} value={collection.id}>
                  {collection.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : null}
        <Button
          className="h-8 gap-1.5 bg-[var(--app-accent)] px-3 text-xs text-[var(--app-accent-fg)] hover:bg-[var(--app-accent)]/90"
          onClick={save}
          disabled={!canEditVariables || saving}
        >
          <Save className="size-3.5" />
          {saving ? "Saving" : "Save"}
        </Button>
      </div>
      <div
        className="overflow-hidden border bg-[var(--app-panel)]"
        style={{
          borderColor: "var(--app-line)",
          borderRadius: "var(--app-radius-lg)",
        }}
      >
        <div className="grid h-9 grid-cols-[32px_minmax(140px,.8fr)_minmax(180px,1.2fr)_86px_72px] items-center border-b border-[var(--app-line)] bg-[rgb(255_255_255/.02)] px-1 text-[11px] uppercase tracking-[0.06em] text-[var(--app-dim)]">
          <div />
          <div>Key</div>
          <div>Value</div>
          <div>Secret</div>
          <div />
        </div>
        {variables.map((variable, index) => (
          <div
            key={index}
            className="grid grid-cols-[32px_minmax(140px,.8fr)_minmax(180px,1.2fr)_86px_72px] items-center border-b border-[var(--app-line)] px-1 last:border-b-0"
          >
            <input
              type="checkbox"
              className="mx-auto size-3 accent-[var(--app-accent)]"
              checked={variable.enabled}
              onChange={(event) =>
                updateVariable(index, { enabled: event.target.checked })
              }
            />
            <Input
              className="app-mono h-8 rounded-none border-0 bg-transparent text-xs shadow-none focus-visible:ring-0"
              value={variable.key}
              onChange={(event) =>
                updateVariable(index, { key: event.target.value })
              }
            />
            <Input
              className="app-mono h-8 rounded-none border-0 bg-transparent text-xs shadow-none focus-visible:ring-0"
              type={
                variable.sensitive && !revealed[index] ? "password" : "text"
              }
              value={variable.value}
              onChange={(event) =>
                updateVariable(index, { value: event.target.value })
              }
            />
            <input
              type="checkbox"
              className="mx-auto size-3 accent-[var(--app-accent)]"
              checked={variable.sensitive}
              onChange={(event) =>
                updateVariable(index, { sensitive: event.target.checked })
              }
            />
            <div className="flex items-center justify-end gap-1 pr-1">
              {variable.sensitive ? (
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-7 text-[var(--app-dim)]"
                  onClick={() =>
                    setRevealed((current) => ({
                      ...current,
                      [index]: !current[index],
                    }))
                  }
                  title={revealed[index] ? "Hide value" : "Reveal value"}
                >
                  {revealed[index] ? (
                    <EyeOff className="size-3.5" />
                  ) : (
                    <Eye className="size-3.5" />
                  )}
                </Button>
              ) : null}
              <Button
                variant="ghost"
                size="icon"
                className="size-7 text-[var(--app-dim)] hover:text-destructive"
                onClick={() => removeVariable(index)}
                title="Remove variable"
              >
                <Trash2 className="size-3.5" />
              </Button>
            </div>
          </div>
        ))}
        <button
          className="flex h-9 w-full items-center gap-2 px-3 text-left text-xs text-[var(--app-dim)] hover:bg-[var(--app-panel-2)] hover:text-[var(--app-text)]"
          onClick={() =>
            canEditVariables &&
            setVariables([
              ...variables,
              {
                scope,
                collectionId,
                environmentId,
                key: "",
                value: "",
                initialValue: null,
                enabled: true,
                sensitive: false,
                variableType: null,
              },
            ])
          }
          disabled={!canEditVariables}
        >
          <Plus className="size-3.5" />
          Add variable
        </button>
      </div>
      {scope === "collection" && !collections.length ? (
        <div className="mt-3 text-xs text-[var(--app-dim)]">
          Import a collection to manage collection variables.
        </div>
      ) : null}
      {scope === "environment" && !environments.length ? (
        <div className="mt-3 text-xs text-[var(--app-dim)]">
          Create or import an environment to manage environment variables.
        </div>
      ) : null}
    </div>
  );
}
