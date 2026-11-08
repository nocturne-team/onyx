import type { Theme } from "./settings/store.svelte";

/** A concrete theme, i.e. anything but "system". Matches `[data-theme]` in app.css. */
export type ThemeId = Exclude<Theme, "system">;

/** Accent ids. Each theme defines `--a-<id>` for the ones it lists. */
export type AccentId =
  | "rosewater"
  | "flamingo"
  | "pink"
  | "purple"
  | "red"
  | "maroon"
  | "orange"
  | "yellow"
  | "green"
  | "teal"
  | "sky"
  | "sapphire"
  | "blue"
  | "lavender";

export interface Accent {
  id: AccentId;
  /** The name the theme itself uses ("Mauve" in Catppuccin, "Aqua" in Gruvbox). */
  label: string;
}

export interface ThemeFamily {
  label: string;
  themes: { id: Theme; label: string }[];
  /** Accent order as shown in the picker; the first is the default. */
  accents: Accent[];
}

const onyxAccents: Accent[] = [
  { id: "green", label: "Green" },
  { id: "teal", label: "Teal" },
  { id: "blue", label: "Blue" },
  { id: "purple", label: "Purple" },
  { id: "pink", label: "Pink" },
  { id: "red", label: "Red" },
  { id: "orange", label: "Orange" },
  { id: "yellow", label: "Yellow" },
];

export const families: ThemeFamily[] = [
  {
    label: "Onyx",
    themes: [
      { id: "dark", label: "Dark" },
      { id: "light", label: "Light" },
      { id: "oled", label: "OLED" },
      { id: "system", label: "System" },
    ],
    accents: onyxAccents,
  },
  {
    label: "Catppuccin",
    themes: [
      { id: "catppuccin-latte", label: "Latte" },
      { id: "catppuccin-frappe", label: "Frappé" },
      { id: "catppuccin-macchiato", label: "Macchiato" },
      { id: "catppuccin-mocha", label: "Mocha" },
    ],
    accents: [
      { id: "purple", label: "Mauve" },
      { id: "rosewater", label: "Rosewater" },
      { id: "flamingo", label: "Flamingo" },
      { id: "pink", label: "Pink" },
      { id: "red", label: "Red" },
      { id: "maroon", label: "Maroon" },
      { id: "orange", label: "Peach" },
      { id: "yellow", label: "Yellow" },
      { id: "green", label: "Green" },
      { id: "teal", label: "Teal" },
      { id: "sky", label: "Sky" },
      { id: "sapphire", label: "Sapphire" },
      { id: "blue", label: "Blue" },
      { id: "lavender", label: "Lavender" },
    ],
  },
  {
    label: "Gruvbox",
    themes: [
      { id: "gruvbox-dark", label: "Dark" },
      { id: "gruvbox-light", label: "Light" },
    ],
    accents: [
      { id: "orange", label: "Orange" },
      { id: "red", label: "Red" },
      { id: "yellow", label: "Yellow" },
      { id: "green", label: "Green" },
      { id: "teal", label: "Aqua" },
      { id: "blue", label: "Blue" },
      { id: "purple", label: "Purple" },
    ],
  },
  {
    label: "Ayu",
    themes: [
      { id: "ayu-dark", label: "Dark" },
      { id: "ayu-mirage", label: "Mirage" },
      { id: "ayu-light", label: "Light" },
    ],
    accents: [
      { id: "yellow", label: "Yellow" },
      { id: "orange", label: "Orange" },
      { id: "red", label: "Red" },
      { id: "green", label: "Green" },
      { id: "teal", label: "Teal" },
      { id: "sky", label: "Cyan" },
      { id: "blue", label: "Blue" },
      { id: "purple", label: "Purple" },
    ],
  },
];

export function familyOf(theme: Theme): ThemeFamily {
  return families.find((f) => f.themes.some((t) => t.id === theme)) ?? families[0];
}

/** What "system" resolves to. */
export function resolveTheme(theme: Theme, prefersDark: boolean): ThemeId {
  return theme === "system" ? (prefersDark ? "dark" : "light") : theme;
}
