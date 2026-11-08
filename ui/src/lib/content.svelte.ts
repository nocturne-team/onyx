import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { loadInstances, type Instance } from "./instances.svelte";

/** Mirrors `onyx_core::content::InstalledFile`. */
export interface InstalledFile {
  projectId: string;
  fileName: string;
  versionNumber: string;
}

/** Mirrors `onyx_core::content::Update`. */
export interface Update {
  key: string;
  projectId: string;
  current: string;
  latest: { id: string; versionNumber: string };
}

export function checkUpdates(instanceId: string): Promise<Update[]> {
  return invoke<Update[]>("check_updates", { id: instanceId });
}

export function applyUpdate(instanceId: string, update: Update): Promise<InstalledFile> {
  return invoke<InstalledFile>("apply_update", {
    id: instanceId,
    key: update.key,
    versionId: update.latest.id,
  });
}

export interface PackUpdate {
  versionId: string;
  versionNumber: string;
}

export function checkPackUpdate(instanceId: string): Promise<PackUpdate | null> {
  return invoke<PackUpdate | null>("check_pack_update", { id: instanceId });
}

export async function updateModpack(instanceId: string, versionId: string) {
  await invoke("update_modpack", { id: instanceId, versionId });
}

/** Mirrors `content::KnownFile` in src-tauri. */
export interface KnownFile {
  projectId: string;
  title: string;
  iconUrl: string | null;
  versionNumber: string;
}

/**
 * Install state per "<instance id>/<project id>", so buttons in Browse and
 * the install popup agree.
 */
export const installs = $state<Record<string, "installing" | "installed" | { error: string }>>({});

export const installKey = (instanceId: string, projectId: string) => `${instanceId}/${projectId}`;

export async function installToInstance(instanceId: string, projectId: string, projectType: string) {
  const key = installKey(instanceId, projectId);
  installs[key] = "installing";
  try {
    await invoke<InstalledFile[]>("install_to_instance", { id: instanceId, projectId, projectType });
    installs[key] = "installed";
  } catch (e) {
    const error = String(e);
    installs[key] = error.includes("already installed") ? "installed" : { error };
  }
}

/** Mods, packs and shaders can't all go everywhere. */
export function canInstallInto(instance: Instance, projectType: string): boolean {
  return projectType !== "mod" || instance.loader !== "vanilla";
}

export function identifyContent(instanceId: string): Promise<Record<string, KnownFile>> {
  return invoke<Record<string, KnownFile>>("identify_content", { id: instanceId });
}

/** Installs a modpack from Modrinth as a new instance. */
export async function installModpack(projectId: string): Promise<Instance> {
  const instance = await invoke<Instance>("install_modpack", { projectId });
  await loadInstances();
  return instance;
}

/** Asks for an .mrpack file and installs it as a new instance; `null` if cancelled. */
export async function importMrpack(): Promise<Instance | null> {
  const path = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "Modrinth modpack", extensions: ["mrpack"] }],
  });
  if (!path) return null;
  const instance = await invoke<Instance>("import_mrpack", { path });
  await loadInstances();
  return instance;
}
