import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { loadInstances, type Instance } from "./instances.svelte";

/** An instance in a Prism Launcher (or MultiMC) folder. */
export interface PrismInstance {
  /** Folder name in Prism's instances folder; what gets imported. */
  folder: string;
  name: string;
  gameVersion: string | null;
  loader: Instance["loader"] | null;
  loaderVersion: string | null;
  /** Why it can't be imported, if it can't. */
  unsupported: string | null;
  iconPath: string | null;
  /** Unix seconds. */
  lastPlayed: number | null;
  playedSecs: number;
  /** "CurseForge", "Modrinth", ... */
  packPlatform: string | null;
}

/** Prism data folders found in the usual places. */
export async function prismRoots(): Promise<string[]> {
  if (!isTauri()) return [];
  return invoke<string[]>("prism_roots");
}

export async function prismInstances(root: string): Promise<PrismInstance[]> {
  return invoke<PrismInstance[]>("prism_instances", { root });
}

export async function pickPrismFolder(): Promise<string | null> {
  const path = await open({ directory: true, multiple: false, title: "Prism Launcher data folder" });
  return typeof path === "string" ? path : null;
}

/**
 * Creates the instances and starts copying their files in the background.
 * Returns what was created and why any of the picked ones weren't.
 */
export async function importPrismInstances(
  root: string,
  folders: string[],
): Promise<{ instances: Instance[]; errors: string[] }> {
  const result = await invoke<{ instances: Instance[]; errors: string[] }>(
    "import_prism_instances",
    { root, folders },
  );
  await loadInstances();
  return result;
}
