import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { IconConfig } from "./iconCatalog";

/** Mirrors `onyx_core::instance::Instance`. */
export interface Instance {
  id: string;
  name: string;
  gameVersion: string;
  loader: "vanilla" | "fabric" | "quilt" | "forge" | "neoforge";
  loaderVersion: string | null;
  /** Unix seconds. */
  created: number;
  lastPlayed: number | null;
  playedSecs: number;
  maxMemoryMb: number | null;
  /** File name of a custom icon; `null` means a generated one. */
  icon: string | null;
  iconConfig: IconConfig | null;
  /** The Modrinth modpack version this came from, when known. */
  pack: { projectId: string; versionId: string; versionNumber: string } | null;
  /** A Java binary just for this instance. */
  javaPath: string | null;
}

export interface Progress {
  doneFiles: number;
  totalFiles: number;
  doneBytes: number;
  totalBytes: number;
}

/** Mirrors `instances::GameState` in src-tauri. */
export interface GameState {
  id: string;
  phase: "preparing" | "running" | null;
  stage: string | null;
  progress: Progress | null;
  error: string | null;
}

export interface ManifestEntry {
  id: string;
  type: "release" | "snapshot" | "old_beta" | "old_alpha";
  releaseTime: string;
}

export const instances = $state({
  list: [] as Instance[],
  loaded: false,
  /** Instances that are preparing or running, by id. */
  games: {} as Record<string, GameState>,
  /** The last launch error per instance, until the next launch. */
  errors: {} as Record<string, string>,
});

export const loaderNames: Record<Instance["loader"], string> = {
  vanilla: "Vanilla",
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
};

export function instanceById(id: string | null): Instance | undefined {
  return instances.list.find((i) => i.id === id);
}

export async function loadInstances() {
  if (!isTauri()) {
    instances.loaded = true;
    return;
  }
  instances.list = await invoke<Instance[]>("list_instances");
  instances.loaded = true;
}

let listening = false;

/** Loads instances and starts following game state events. Call once. */
export async function initInstances() {
  if (!isTauri() || listening) return loadInstances();
  listening = true;
  listen<GameState>("instance:state", ({ payload }) => {
    if (payload.phase) {
      instances.games[payload.id] = payload;
    } else {
      delete instances.games[payload.id];
      // Play time and last played changed.
      loadInstances();
    }
    if (payload.error) instances.errors[payload.id] = payload.error;
  });
  for (const game of await invoke<GameState[]>("running_games")) {
    instances.games[game.id] = game;
  }
  await loadInstances();
}

export async function createInstance(
  name: string,
  gameVersion: string,
  loader: Instance["loader"],
  loaderVersion: string | null,
  iconConfig: IconConfig | null,
  iconPath: string | null,
): Promise<Instance> {
  const instance = await invoke<Instance>("create_instance", {
    name,
    gameVersion,
    loader,
    loaderVersion,
    iconConfig,
    iconPath,
  });
  await loadInstances();
  return instance;
}

export async function deleteInstance(id: string) {
  await invoke("delete_instance", { id });
  await loadInstances();
}

export async function launch(id: string) {
  delete instances.errors[id];
  instances.games[id] = { id, phase: "preparing", stage: null, progress: null, error: null };
  try {
    await invoke("launch_instance", { id });
  } catch (e) {
    delete instances.games[id];
    instances.errors[id] = String(e);
  }
}

export function stop(id: string) {
  return invoke("stop_game", { id });
}

let versions: Promise<ManifestEntry[]> | null = null;

/** Minecraft versions, newest first. Fetched once per app run. */
export function gameVersions(): Promise<ManifestEntry[]> {
  versions ??= invoke<{ versions: ManifestEntry[] }>("game_versions")
    .then((m) => m.versions)
    .catch((e) => {
      versions = null;
      throw e;
    });
  return versions;
}

export interface LoaderVersion {
  version: string;
  stable: boolean;
}

const loaderGames = new Map<string, Promise<Set<string>>>();

/** Game versions a mod loader supports. Fetched once per app run. */
export function loaderGameVersions(loader: Instance["loader"]): Promise<Set<string>> {
  let cached = loaderGames.get(loader);
  if (!cached) {
    cached = invoke<string[]>("loader_game_versions", { loader }).then((v) => new Set(v));
    cached.catch(() => loaderGames.delete(loader));
    loaderGames.set(loader, cached);
  }
  return cached;
}

/** Loader versions for a game version, newest first. */
export function loaderVersions(
  loader: Instance["loader"],
  gameVersion: string,
): Promise<LoaderVersion[]> {
  return invoke<LoaderVersion[]>("loader_versions", { loader, gameVersion });
}

export type ContentKind = "mods" | "resourcepacks" | "shaderpacks";

/** Mirrors `onyx_core::mods::ContentFile`. */
export interface ContentFile {
  fileName: string;
  enabled: boolean;
  isDir: boolean;
  size: number;
  modId: string | null;
  name: string | null;
  version: string | null;
  description: string | null;
}

/** Custom icon data URLs by "<id>/<icon file>"; the file name changes on upload. */
const iconUrls = $state<Record<string, string | null>>({});
const iconRequests = new Set<string>();

/** The custom icon's data URL, loaded on first use; `undefined` while loading. */
export function customIconUrl(instance: Instance): string | null | undefined {
  if (!instance.icon) return null;
  const key = `${instance.id}/${instance.icon}`;
  if (!(key in iconUrls) && !iconRequests.has(key) && isTauri()) {
    iconRequests.add(key);
    invoke<string | null>("instance_icon", { id: instance.id })
      .then((url) => (iconUrls[key] = url))
      .catch(() => (iconUrls[key] = null));
  }
  return iconUrls[key];
}

/** Asks for an image file. `null` if cancelled. */
export async function pickIconFile(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const path = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
  });
  return typeof path === "string" ? path : null;
}

export function imagePreview(path: string): Promise<string | null> {
  return invoke<string | null>("image_preview", { path });
}

export async function setInstanceIcon(id: string, path: string) {
  await invoke("set_instance_icon", { id, path });
  await loadInstances();
}

export async function setInstanceMemory(id: string, maxMemoryMb: number | null) {
  await invoke("set_instance_memory", { id, maxMemoryMb });
  await loadInstances();
}

export async function setInstanceJava(id: string, javaPath: string | null) {
  await invoke("set_instance_java", { id, javaPath });
  await loadInstances();
}

export async function setInstanceIconConfig(id: string, config: IconConfig) {
  await invoke("set_instance_icon_config", { id, config });
  await loadInstances();
}
