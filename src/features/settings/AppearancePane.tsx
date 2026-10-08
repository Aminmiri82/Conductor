import { Check } from "lucide-react";
import type { AppTheme, UrlDisplayMode } from "@/features/types";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";
import { SectionTitle, SettingRow, Segmented } from "./SettingsParts";

// Colours come from the `[data-theme]` blocks in index.css. A preview card sets
// `data-theme` on itself and reads the same variables the real theme uses.
// `defaultAccent` is the one value kept here: picking a theme resets the
// accent to it.
const THEMES: {
  id: AppTheme;
  name: string;
  mode: string;
  defaultAccent: string;
}[] = [
  {
    id: "softpro",
    name: "Soft Pro",
    mode: "Dark · Default",
    defaultAccent: "#a78bfa",
  },
  {
    id: "conductor",
    name: "Train Conductor",
    mode: "Dark · Bold",
    defaultAccent: "#ffb454",
  },
  {
    id: "brutalist",
    name: "Brutalist",
    mode: "Dark · Mono",
    defaultAccent: "#fff09b",
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

export function AppearancePane() {
  const theme = useWorkspaceUiStore((state) => state.workspaceUi.appTheme);
  const accent = useWorkspaceUiStore((state) => state.workspaceUi.accentColor);
  const urlMode = useWorkspaceUiStore(
    (state) => state.workspaceUi.urlDisplayMode,
  );
  const setWorkspacePreference = useWorkspaceUiStore(
    (state) => state.setWorkspacePreference,
  );

  function setTheme(next: (typeof THEMES)[number]) {
    setWorkspacePreference("appTheme", next.id);
    setWorkspacePreference("accentColor", next.defaultAccent);
  }

  return (
    <div>
      <SectionTitle
        title="Theme"
        sub="Pick the visual direction for the whole workspace."
      />
      <div className="mb-6 grid grid-cols-1 gap-3 md:grid-cols-3">
        {THEMES.map((item) => (
          // The button itself stays on the live theme (border, radius); only
          // the two inner blocks switch to the previewed theme.
          <button
            key={item.id}
            className="overflow-hidden border text-left"
            style={{
              borderRadius: "var(--app-radius-lg)",
              borderColor:
                theme === item.id ? "var(--app-accent)" : "var(--app-line)",
            }}
            onClick={() => setTheme(item)}
          >
            <div
              data-theme={item.id}
              className="flex h-24 flex-col gap-1.5 bg-[var(--app-bg)] p-2.5"
            >
              <div className="h-2 rounded-[2px] bg-[var(--app-panel-2)]" />
              <div className="flex flex-1 gap-1.5">
                <div className="w-16 rounded-[2px] bg-[var(--app-panel-2)]" />
                <div className="flex flex-1 flex-col gap-1 pt-1">
                  <div className="h-1.5 w-4/5 bg-current opacity-30" />
                  <div className="h-1.5 w-2/5 bg-[var(--app-accent)]" />
                  <div className="h-1.5 w-3/5 bg-current opacity-25" />
                </div>
              </div>
            </div>
            <div className="border-t border-[var(--app-line)]">
              <div
                data-theme={item.id}
                className="flex items-center justify-between bg-[var(--app-panel-2)] px-2.5 py-2"
              >
                <div>
                  <div className="text-xs font-semibold text-white">
                    {item.name}
                  </div>
                  <div className="text-[10px] uppercase tracking-[0.04em] text-[var(--app-dim)]">
                    {item.mode}
                  </div>
                </div>
                {theme === item.id ? (
                  <span
                    className="grid size-4 place-items-center rounded-full text-[10px] font-bold text-[var(--app-accent-fg)]"
                    style={{ background: accent }}
                  >
                    <Check className="size-3" />
                  </span>
                ) : null}
              </div>
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
                boxShadow: accent === item.id ? `0 0 0 2px ${item.id}` : "none",
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
