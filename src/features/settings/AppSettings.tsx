import { useEffect, useState } from "react";
import { Check, Eye, EyeOff, Plus, Save, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";
import { api } from "@/lib/tauri";
import type {
  AppTheme,
  SettingsTab,
  UrlDisplayMode,
  VariableEntry,
} from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

const SETTINGS_TABS: { id: SettingsTab; label: string }[] = [
  { id: "appearance", label: "Appearance" },
  { id: "variables", label: "Variables" },
  { id: "shortcuts", label: "Shortcuts" },
  { id: "about", label: "About" },
];

const THEMES: {
  id: AppTheme;
  name: string;
  mode: string;
  defaultAccent: string;
  palette: { bg: string; panel: string; accent: string; text: string; dim: string };
}[] = [
  {
    id: "softpro",
    name: "Soft Pro",
    mode: "Dark · Default",
    defaultAccent: "#a78bfa",
    palette: {
      bg: "#121016",
      panel: "#1c1822",
      accent: "#a78bfa",
      text: "#fff",
      dim: "#9b9ba2",
    },
  },
  {
    id: "conductor",
    name: "Train Conductor",
    mode: "Dark · Bold",
    defaultAccent: "#ffb454",
    palette: {
      bg: "#0b1018",
      panel: "#0f1622",
      accent: "#ffb454",
      text: "#fff",
      dim: "#7e8595",
    },
  },
  {
    id: "brutalist",
    name: "Brutalist",
    mode: "Dark · Mono",
    defaultAccent: "#fff09b",
    palette: {
      bg: "#0a0a0a",
      panel: "#101010",
      accent: "#fff09b",
      text: "#fff",
      dim: "#9a9a9a",
    },
  },
];

const ACCENTS = [
  { id: "#a78bfa", name: "Purple" },
  { id: "#ffb454", name: "Amber" },
  { id: "#7eedb8", name: "Signal" },
  { id: "#7dd3fc", name: "Sky" },
  { id: "#ff7a6b", name: "Coral" },
  { id: "#e8e8ea", name: "Mono" },
];

export function AppSettings({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const workspaceUi = useWorkspaceStore((state) => state.workspaceUi);
  const setWorkspacePreference = useWorkspaceStore(
    (state) => state.setWorkspacePreference,
  );
  const activeCollectionId = useWorkspaceStore((state) => state.activeCollectionId);

  useEffect(() => {
    if (!open) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onOpenChange(false);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onOpenChange, open]);

  if (!open) return null;

  const tab = workspaceUi.settingsTab;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-4 backdrop-blur-sm"
      onMouseDown={() => onOpenChange(false)}
    >
      <section
        className="flex h-[min(640px,88vh)] w-[min(960px,94vw)] min-w-0 flex-col overflow-hidden border border-[var(--app-line)] bg-[var(--app-bg)] text-[var(--app-text)] shadow-[0_30px_80px_rgba(0,0,0,.6)]"
        style={{ borderRadius: "var(--app-radius-lg)" }}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="flex h-11 shrink-0 items-center gap-3 border-b border-[var(--app-line)] bg-[var(--app-panel)] px-3">
          <div className="text-sm font-semibold">Settings</div>
          <kbd className="app-mono rounded border border-[var(--app-line)] bg-[var(--app-panel-2)] px-1.5 py-0.5 text-[11px] text-[var(--app-dim)]">
            ⌘,
          </kbd>
          <div className="flex-1" />
          <Button
            variant="ghost"
            size="icon"
            className="size-7 text-[var(--app-dim)] hover:text-[var(--app-text)]"
            onClick={() => onOpenChange(false)}
          >
            <X className="size-3.5" />
          </Button>
        </div>
        <div className="flex min-h-0 flex-1">
          <nav className="w-52 shrink-0 border-r border-[var(--app-line)] bg-[var(--app-panel)] p-2">
            {SETTINGS_TABS.map((item) => (
              <button
                key={item.id}
                className={cn(
                  "mb-1 flex h-8 w-full items-center px-2.5 text-left text-xs font-medium text-[var(--app-dim)] hover:bg-[var(--app-panel-2)] hover:text-[var(--app-text)]",
                  tab === item.id &&
                    "border border-[color-mix(in_oklab,var(--app-accent)_28%,transparent)] bg-[color-mix(in_oklab,var(--app-accent)_14%,transparent)] text-[var(--app-text)]",
                )}
                style={{ borderRadius: "var(--app-radius)" }}
                onClick={() => setWorkspacePreference("settingsTab", item.id)}
              >
                {item.label}
              </button>
            ))}
          </nav>
          <ScrollArea className="min-w-0 flex-1">
            <div className="p-6">
              {tab === "appearance" ? <AppearancePane /> : null}
              {tab === "variables" ? (
                <VariablesPane activeCollectionId={activeCollectionId} />
              ) : null}
              {tab === "shortcuts" ? <ShortcutsPane /> : null}
              {tab === "about" ? <AboutPane /> : null}
            </div>
          </ScrollArea>
        </div>
      </section>
    </div>
  );
}

function AppearancePane() {
  const theme = useWorkspaceStore((state) => state.workspaceUi.appTheme);
  const accent = useWorkspaceStore((state) => state.workspaceUi.accentColor);
  const urlMode = useWorkspaceStore((state) => state.workspaceUi.urlDisplayMode);
  const setWorkspacePreference = useWorkspaceStore(
    (state) => state.setWorkspacePreference,
  );

  function setTheme(nextTheme: AppTheme) {
    const next = THEMES.find((item) => item.id === nextTheme);
    setWorkspacePreference("appTheme", nextTheme);
    if (next) setWorkspacePreference("accentColor", next.defaultAccent);
  }

  return (
    <div>
      <SectionTitle
        title="Theme"
        sub="Pick the visual direction for the whole workspace."
      />
      <div className="mb-6 grid grid-cols-1 gap-3 md:grid-cols-3">
        {THEMES.map((item) => (
          <button
            key={item.id}
            className="overflow-hidden border text-left"
            style={{
              borderRadius: "var(--app-radius-lg)",
              borderColor:
                theme === item.id ? "var(--app-accent)" : "var(--app-line)",
              background: item.palette.bg,
            }}
            onClick={() => setTheme(item.id)}
          >
            <div
              className="flex h-24 flex-col gap-1.5 p-2.5"
              style={{ background: item.palette.bg }}
            >
              <div
                className="h-2"
                style={{
                  borderRadius: 2,
                  background: item.palette.panel,
                }}
              />
              <div className="flex flex-1 gap-1.5">
                <div
                  className="w-16"
                  style={{
                    borderRadius: 2,
                    background: item.palette.panel,
                  }}
                />
                <div className="flex flex-1 flex-col gap-1 pt-1">
                  <div className="h-1.5 w-4/5 bg-current opacity-30" />
                  <div
                    className="h-1.5 w-2/5"
                    style={{ background: item.palette.accent }}
                  />
                  <div className="h-1.5 w-3/5 bg-current opacity-25" />
                </div>
              </div>
            </div>
            <div
              className="flex items-center justify-between border-t px-2.5 py-2"
              style={{
                borderColor: "var(--app-line)",
                background: item.palette.panel,
              }}
            >
              <div>
                <div
                  className="text-xs font-semibold"
                  style={{ color: item.palette.text }}
                >
                  {item.name}
                </div>
                <div
                  className="text-[10px] uppercase tracking-[0.04em]"
                  style={{ color: item.palette.dim }}
                >
                  {item.mode}
                </div>
              </div>
              {theme === item.id ? (
                <span className="grid size-4 place-items-center rounded-full bg-[var(--app-accent)] text-[10px] font-bold text-[var(--app-accent-fg)]">
                  <Check className="size-3" />
                </span>
              ) : null}
            </div>
          </button>
        ))}
      </div>

      <SectionTitle title="Accent" />
      <div className="mb-2 flex flex-wrap gap-5 border-b border-[var(--app-line)] pb-5">
        {ACCENTS.map((item) => (
          <button
            key={item.id}
            className="flex flex-col items-center gap-1.5 text-[11px] text-[var(--app-dim)]"
            onClick={() => setWorkspacePreference("accentColor", item.id)}
          >
            <span
              className="size-10"
              style={{
                borderRadius: "var(--app-radius-lg)",
                background: item.id,
                border:
                  accent === item.id
                    ? "2px solid var(--app-text)"
                    : "2px solid transparent",
                boxShadow:
                  accent === item.id ? `0 0 0 2px ${item.id}` : "none",
              }}
            />
            {item.name}
          </button>
        ))}
      </div>

      <SettingRow
        label="URL bar style"
        hint="How variables are rendered while editing request URLs."
      >
        <Segmented
          value={urlMode}
          options={[
            ["flat", "Flat"],
            ["syntax", "Syntax"],
            ["chip", "Chips"],
            ["hybrid", "Hybrid"],
          ]}
          onChange={(value) =>
            setWorkspacePreference("urlDisplayMode", value as UrlDisplayMode)
          }
        />
      </SettingRow>
    </div>
  );
}

function VariablesPane({
  activeCollectionId,
}: {
  activeCollectionId?: string;
}) {
  const collections = useWorkspaceStore((state) => state.collections);
  const [scope, setScope] = useState<"collection" | "global">(
    activeCollectionId || collections[0] ? "collection" : "global",
  );
  const [selectedCollectionId, setSelectedCollectionId] = useState(
    activeCollectionId ?? collections[0]?.id ?? "",
  );
  const [variables, setVariables] = useState<VariableEntry[]>([]);
  const [saving, setSaving] = useState(false);
  const [revealed, setRevealed] = useState<Record<number, boolean>>({});
  const resolveActiveRequest = useWorkspaceStore((state) => state.resolveActiveRequest);

  const scopeId = scope === "collection" ? selectedCollectionId : "global";

  useEffect(() => {
    if (selectedCollectionId || !collections[0]) return;
    setSelectedCollectionId(collections[0].id);
  }, [collections, selectedCollectionId]);

  useEffect(() => {
    let mounted = true;
    if (!scopeId) {
      setVariables([]);
      return;
    }
    void api.listVariables(scope, scopeId).then((items) => {
      if (mounted) setVariables(items);
    });
    return () => {
      mounted = false;
    };
  }, [scope, scopeId]);

  async function save() {
    if (!scopeId) return;
    setSaving(true);
    await api.saveVariables(
      scope,
      scopeId,
      variables.map((variable) => ({ ...variable, scopeKind: scope, scopeId })),
    );
    setSaving(false);
    await resolveActiveRequest();
  }

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-start gap-3">
        <SectionTitle
          title="Variables"
          sub="Resolution order is Collection → Global."
        />
        <div className="flex-1" />
        <Segmented
          value={scope}
          options={[
            ["collection", "Collection"],
            ["global", "Global"],
          ]}
          onChange={(value) => setScope(value as typeof scope)}
        />
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
          disabled={!scopeId || saving}
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
              onChange={(event) => updateVariable(index, { key: event.target.value })}
            />
            <Input
              className="app-mono h-8 rounded-none border-0 bg-transparent text-xs shadow-none focus-visible:ring-0"
              type={variable.sensitive && !revealed[index] ? "password" : "text"}
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
            scopeId &&
            setVariables([
              ...variables,
              {
                scopeKind: scope,
                scopeId,
                key: "",
                value: "",
                enabled: true,
                sensitive: false,
              },
            ])
          }
          disabled={!scopeId}
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

function ShortcutsPane() {
  const groups: { title: string; rows: [string, string[]][] }[] = [
    {
      title: "Navigation",
      rows: [
        ["Toggle sidebar", ["⌘", "B"]],
        ["Open settings", ["⌘", ","]],
        ["Focus URL", ["⌘", "L"]],
      ],
    },
    {
      title: "Requests",
      rows: [
        ["New request", ["⌘", "N"]],
        ["Send", ["⌘", "↵"]],
        ["Save", ["⌘", "S"]],
        ["Duplicate tab", ["⌘", "D"]],
        ["Close tab", ["⌘", "W"]],
      ],
    },
  ];

  return (
    <div>
      <SectionTitle title="Keyboard Shortcuts" sub="Core app shortcuts." />
      {groups.map((group) => (
        <div key={group.title} className="mb-5">
          <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.09em] text-[var(--app-dim)]">
            {group.title}
          </div>
          <div
            className="overflow-hidden border"
            style={{
              borderColor: "var(--app-line)",
              borderRadius: "var(--app-radius-lg)",
            }}
          >
            {group.rows.map(([label, keys], index) => (
              <div
                key={label}
                className="flex h-10 items-center border-b border-[var(--app-line)] px-3 last:border-b-0"
              >
                <div className="flex-1 text-sm">{label}</div>
                <div className="flex gap-1">
                  {keys.map((key) => (
                    <kbd
                      key={key}
                      className="app-mono min-w-6 rounded border border-[var(--app-line)] bg-[var(--app-panel-2)] px-1.5 py-0.5 text-center text-[11px] text-[var(--app-text)]"
                    >
                      {key}
                    </kbd>
                  ))}
                </div>
                <span className="sr-only">{index}</span>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function AboutPane() {
  return (
    <div>
      <SectionTitle title="Conductor" sub="API workspace for collections and requests." />
      <div className="app-mono text-xs leading-6 text-[var(--app-dim)]">
        Version 0.1.0
      </div>
    </div>
  );
}

function SectionTitle({ title, sub }: { title: string; sub?: string }) {
  return (
    <div className="mb-3">
      <div className="text-sm font-semibold text-[var(--app-text)]">{title}</div>
      {sub ? (
        <div className="mt-1 text-xs text-[var(--app-dim)]">{sub}</div>
      ) : null}
    </div>
  );
}

function SettingRow({
  label,
  hint,
  children,
}: {
  label: string;
  hint: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 border-b border-[var(--app-line)] py-4">
      <div>
        <div className="text-sm font-medium text-[var(--app-text)]">{label}</div>
        <div className="mt-0.5 text-xs text-[var(--app-dim)]">{hint}</div>
      </div>
      {children}
    </div>
  );
}

function Segmented({
  value,
  options,
  onChange,
}: {
  value: string;
  options: readonly (readonly [string, string])[];
  onChange: (value: string) => void;
}) {
  return (
    <div
      className="flex shrink-0 border bg-[var(--app-panel-2)] p-0.5 text-xs"
      style={{
        borderColor: "var(--app-line)",
        borderRadius: "var(--app-radius)",
      }}
    >
      {options.map(([id, label]) => (
        <button
          key={id}
          className={cn(
            "px-2.5 py-1 font-medium text-[var(--app-dim)]",
            value === id &&
              "bg-[var(--app-accent)] text-[var(--app-accent-fg)]",
          )}
          style={{ borderRadius: "var(--app-radius)" }}
          onClick={() => onChange(id)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
