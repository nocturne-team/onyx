import { invoke, isTauri } from "@tauri-apps/api/core";

export type Theme = "dark" | "light" | "oled" | "system";

/** Mirrors `onyx_core::Settings`. */
export interface Settings {
  theme: Theme;
  advancedRendering: boolean;
  /** Megabytes; null means Mojang's recommendation per version. */
  defaultMemoryMb: number | null;
}

export interface AppInfo {
  version: string;
  os: string;
  totalMemoryMb: number | null;
}

export const settings = $state<Settings>({
  theme: "dark",
  advancedRendering: true,
  defaultMemoryMb: null,
});

export async function loadSettings() {
  if (!isTauri()) return;
  Object.assign(settings, await invoke<Settings>("get_settings"));
}

export async function updateSettings(patch: Partial<Settings>) {
  Object.assign(settings, patch);
  if (!isTauri()) return;
  try {
    await invoke("set_settings", { settings: $state.snapshot(settings) });
  } catch (e) {
    console.error("failed to save settings", e);
  }
}

export async function appInfo(): Promise<AppInfo> {
  if (!isTauri()) return { version: "dev", os: "browser", totalMemoryMb: null };
  return invoke<AppInfo>("app_info");
}
