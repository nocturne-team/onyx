import { invoke, isTauri } from "@tauri-apps/api/core";

/** Mirrors `onyx_core::auth::microsoft::MinecraftAccount`. */
export interface MinecraftAccount {
  id: string;
  name: string;
  skinUrl: string | null;
  offline?: boolean;
}

export const minecraft = $state({
  accounts: [] as MinecraftAccount[],
  active: null as string | null,
  signingIn: false,
  error: null as string | null,
});

export function activeAccount(): MinecraftAccount | undefined {
  return minecraft.accounts.find((a) => a.id === minecraft.active);
}

export async function loadMinecraftAccounts() {
  if (!isTauri()) return;
  const result = await invoke<{ active: string | null; accounts: MinecraftAccount[] }>(
    "minecraft_accounts",
  );
  minecraft.accounts = result.accounts;
  minecraft.active = result.active;
}

export async function addMinecraftAccount() {
  minecraft.signingIn = true;
  minecraft.error = null;
  try {
    await invoke<MinecraftAccount>("login_minecraft");
    await loadMinecraftAccounts();
  } catch (e) {
    const message = String(e);
    if (message !== "login cancelled") minecraft.error = message;
  } finally {
    minecraft.signingIn = false;
  }
}

/** Offline accounts need a Microsoft one first; the backend checks too. */
export function canAddOffline(): boolean {
  return minecraft.accounts.some((a) => !a.offline);
}

export async function addOfflineAccount(name: string): Promise<boolean> {
  minecraft.error = null;
  try {
    await invoke<MinecraftAccount>("add_offline_account", { name });
    await loadMinecraftAccounts();
    return true;
  } catch (e) {
    minecraft.error = String(e);
    return false;
  }
}

export function cancelMinecraftLogin() {
  invoke("cancel_minecraft_login");
}

export async function setActiveAccount(id: string) {
  await invoke("set_active_minecraft_account", { id });
  minecraft.active = id;
}

export async function removeAccount(id: string) {
  await invoke("remove_minecraft_account", { id });
  await loadMinecraftAccounts();
}
