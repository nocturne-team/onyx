import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Mirrors `onyx_core::modrinth::User`. */
export interface User {
  id: string;
  username: string;
  avatarUrl: string | null;
}

export const account = $state({
  user: null as User | null,
  /** "restoring" until the saved token has been checked at startup. */
  status: "restoring" as "restoring" | "signed-out" | "signing-in" | "signed-in",
  error: null as string | null,
});

function signedIn(user: User) {
  account.user = user;
  account.status = "signed-in";
  account.error = null;
}

function signedOut(error: string | null = null) {
  account.user = null;
  account.status = "signed-out";
  account.error = error;
}

export async function restoreSession() {
  if (!isTauri()) return signedOut();
  listen("account:signed-out", () => signedOut("Your Modrinth login expired. Sign in again."));
  try {
    const user = await invoke<User | null>("restore_session");
    if (user) signedIn(user);
    else signedOut();
  } catch (e) {
    signedOut(`Couldn't restore your Modrinth login: ${e}`);
  }
}

export async function signInWithBrowser() {
  account.status = "signing-in";
  account.error = null;
  try {
    signedIn(await invoke<User>("login_modrinth"));
  } catch (e) {
    const message = String(e);
    signedOut(message === "login cancelled" ? null : message);
  }
}

export function cancelSignIn() {
  invoke("cancel_login");
}

export async function signInWithToken(token: string) {
  account.status = "signing-in";
  account.error = null;
  try {
    signedIn(await invoke<User>("login_with_token", { token }));
  } catch (e) {
    signedOut(String(e));
  }
}

export async function signOut() {
  try {
    await invoke("logout");
  } finally {
    signedOut();
  }
}
