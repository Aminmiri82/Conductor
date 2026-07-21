import type { AppTheme, SettingsTab } from "@/features/types";

export const SETTINGS_TABS: { id: SettingsTab; label: string }[] = [
  { id: "appearance", label: "Appearance" },
  { id: "variables", label: "Variables" },
  { id: "shortcuts", label: "Shortcuts" },
  { id: "data", label: "Data" },
  { id: "about", label: "About" },
];

export type ThemeDefinition = {
  id: AppTheme;
  name: string;
  mode: string;
  defaultAccent: string;
  palette: {
    bg: string;
    panel: string;
    accent: string;
    text: string;
    dim: string;
  };
};

export const THEMES: ThemeDefinition[] = [
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

export const ACCENTS: { id: string; name: string }[] = [
  { id: "#a78bfa", name: "Purple" },
  { id: "#ffb454", name: "Amber" },
  { id: "#7eedb8", name: "Signal" },
  { id: "#7dd3fc", name: "Sky" },
  { id: "#ff7a6b", name: "Coral" },
  { id: "#e8e8ea", name: "Mono" },
];
