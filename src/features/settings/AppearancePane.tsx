import { Check } from "lucide-react";
import type { AppTheme, UrlDisplayMode } from "@/features/types";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";
import { ACCENTS, THEMES } from "@/features/settings/settingsData";
import {
  SectionTitle,
  Segmented,
  SettingRow,
} from "@/features/settings/settingsShared";

export function AppearancePane() {
  const theme = useWorkspaceUiStore((state) => state.workspaceUi.appTheme);
  const accent = useWorkspaceUiStore((state) => state.workspaceUi.accentColor);
  const urlMode = useWorkspaceUiStore(
    (state) => state.workspaceUi.urlDisplayMode,
  );
  const setWorkspacePreference = useWorkspaceUiStore(
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
