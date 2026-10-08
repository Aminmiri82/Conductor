import { type ChangeEvent, useEffect, useMemo, useRef, useState } from "react";
import { Eye, EyeOff, Plus, Save, Trash2, Upload } from "lucide-react";
import {
  CellInput,
  CheckboxCell,
  HeaderRow,
  RemoveButton,
  Row,
  TableFrame,
} from "@/components/EditableTable";
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
import type {
  VariableEntry,
  VariableScope,
  VariableTarget,
} from "@/features/types";
import {
  diffVariables,
  normalizeVariables,
} from "@/features/variables/variableChanges";
import { useSaveVariables } from "@/features/variables/useSaveVariables";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";
import { Segmented, SectionTitle } from "./SettingsParts";

const columns =
  "grid-cols-[32px_minmax(140px,.8fr)_minmax(180px,1.2fr)_86px_72px]";

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
  const [scope, setScope] = useState<VariableScope>(
    activeCollectionId || collections[0] ? "collection" : "global",
  );
  const [selectedCollectionId, setSelectedCollectionId] = useState(
    activeCollectionId ?? collections[0]?.id ?? "",
  );
  const [selectedEnvironmentId, setSelectedEnvironmentId] = useState(
    activeEnvironmentId ?? environments[0]?.id ?? "",
  );
  const [variables, setVariables] = useState<VariableEntry[]>([]);
  const { saving, save: saveVariables } = useSaveVariables();
  // What the table was loaded with, so a save sends only what changed.
  const loaded = useRef<{
    target: VariableTarget | null;
    variables: VariableEntry[];
  }>({ target: null, variables: [] });
  // The table still shows the previous target's rows until the new target's
  // load lands; saving before then would write those rows to the new target.
  const [loadedTarget, setLoadedTarget] = useState<VariableTarget | null>(null);
  const [environmentName, setEnvironmentName] = useState("");
  const [revealed, setRevealed] = useState<Record<number, boolean>>({});
  const environmentInputRef = useRef<HTMLInputElement>(null);

  // Null until the scope has the collection or environment it needs.
  const target = useMemo((): VariableTarget | null => {
    if (scope === "global") return { scope };
    if (scope === "collection") {
      return selectedCollectionId
        ? { scope, collectionId: selectedCollectionId }
        : null;
    }
    return selectedEnvironmentId
      ? { scope, environmentId: selectedEnvironmentId }
      : null;
  }, [scope, selectedCollectionId, selectedEnvironmentId]);
  const ready = target !== null && loadedTarget === target;

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
    const show = (variables: VariableEntry[]) => {
      loaded.current = { target, variables };
      setVariables(variables);
      setLoadedTarget(target);
    };
    if (!target) {
      show([]);
      return;
    }
    api.listVariables(target).then(
      (items) => {
        if (mounted) show(items);
      },
      (error) => {
        if (!mounted) return;
        // An empty table, so a save cannot be diffed against another target's rows.
        show([]);
        useWorkspaceStore.getState().setError(error);
      },
    );
    return () => {
      mounted = false;
    };
  }, [target]);

  async function save() {
    if (!target || !ready) return;
    const name = environmentName.trim();
    const currentEnvironmentName = environments.find(
      (environment) => environment.id === selectedEnvironmentId,
    )?.name;
    const renameEnvironment =
      scope === "environment" &&
      selectedEnvironmentId &&
      name &&
      name !== currentEnvironmentName;
    const { upserts, deletes } = diffVariables(
      loaded.current.variables,
      variables,
    );
    const saved = await saveVariables(
      upserts.length || deletes.length ? [{ target, upserts, deletes }] : [],
      renameEnvironment
        ? async () => {
            await api.renameEnvironment(selectedEnvironmentId, name);
            await loadEnvironments();
          }
        : undefined,
    );
    if (saved && loaded.current.target === target) {
      loaded.current = { target, variables: normalizeVariables(variables) };
    }
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
          onChange={setScope}
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
          disabled={!ready || saving}
        >
          <Save className="size-3.5" />
          {saving ? "Saving" : "Save"}
        </Button>
      </div>
      <TableFrame className="bg-[var(--app-panel)]">
        <HeaderRow columns={columns} className="h-9">
          <div />
          <div>Key</div>
          <div>Value</div>
          <div>Secret</div>
          <div />
        </HeaderRow>
        {variables.map((variable, index) => (
          <Row key={index} columns={columns}>
            <CheckboxCell
              checked={variable.enabled}
              onChange={(enabled) => updateVariable(index, { enabled })}
            />
            <CellInput
              value={variable.key}
              onChange={(event) =>
                updateVariable(index, { key: event.target.value })
              }
            />
            <CellInput
              type={
                variable.sensitive && !revealed[index] ? "password" : "text"
              }
              value={variable.value}
              onChange={(event) =>
                updateVariable(index, { value: event.target.value })
              }
            />
            <CheckboxCell
              checked={variable.sensitive}
              onChange={(sensitive) => updateVariable(index, { sensitive })}
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
              <RemoveButton
                className="hover:text-destructive"
                onClick={() => removeVariable(index)}
                title="Remove variable"
              />
            </div>
          </Row>
        ))}
        <button
          className="flex h-9 w-full items-center gap-2 px-3 text-left text-xs text-[var(--app-dim)] hover:bg-[var(--app-panel-2)] hover:text-[var(--app-text)]"
          onClick={() =>
            target &&
            setVariables([
              ...variables,
              { key: "", value: "", enabled: true, sensitive: false },
            ])
          }
          disabled={!target}
        >
          <Plus className="size-3.5" />
          Add variable
        </button>
      </TableFrame>
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
}
