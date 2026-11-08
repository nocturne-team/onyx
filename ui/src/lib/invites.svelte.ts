import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { instances, type Instance } from "./instances.svelte";

/** The e4mc mod on Modrinth; it gives LAN worlds a public address. */
export const E4MC_PROJECT_ID = "qANg5Jrr";

/** Mirrors `onyx_core::invite::Invite`. */
export interface Invite {
  address: string;
  gameVersion: string;
  loader: Instance["loader"];
  pack: { projectId: string; versionId: string | null } | null;
}

export type Fit = "samePackVersion" | "samePack" | "sameVersionAndLoader" | "sameVersion";

/** Mirrors `invites::InviteOffer` in src-tauri. */
export interface InviteOffer {
  invite: Invite | null;
  error: string | null;
  candidates: { id: string; fit: Fit }[];
}

/** Mirrors `invites::Share` in src-tauri. */
export interface Share {
  id: string;
  address: string | null;
  link: string | null;
  players: number;
  lanOpen: boolean;
}

/** Mirrors `onyx_core::discord::JoinRequest`. */
export interface JoinRequest {
  userId: string;
  name: string;
  avatarUrl: string | null;
}

export const invites = $state({
  /** An invite waiting for the user to pick an instance. */
  offer: null as InviteOffer | null,
  /** Friends asking to join over Discord, oldest first. */
  requests: [] as JoinRequest[],
  /** Hosting state by instance id. */
  shares: {} as Record<string, Share>,
});

let listening = false;

async function takeInvite() {
  const offer = await invoke<InviteOffer | null>("take_invite");
  if (offer) invites.offer = offer;
}

/** Follows invites, join requests and hosting state. Call once. */
export function initInvites() {
  if (!isTauri() || listening) return;
  listening = true;
  listen("invite:received", () => takeInvite());
  listen<JoinRequest>("discord:join-request", ({ payload }) => {
    invites.requests = [...invites.requests.filter((r) => r.userId !== payload.userId), payload];
  });
  listen<Share>("instance:share", ({ payload }) => {
    invites.shares[payload.id] = payload;
  });
  listen<{ id: string; phase: string | null }>("instance:state", ({ payload }) => {
    if (!payload.phase) delete invites.shares[payload.id];
  });
  // An invite that opened the app arrived before we listened.
  takeInvite().catch((e) => console.error("failed to read invite", e));
}

export async function loadShare(id: string) {
  if (!isTauri()) return;
  const share = await invoke<Share | null>("game_share", { id });
  if (share) invites.shares[id] = share;
}

/** Starts `id` and joins the invite's server. */
export async function joinInvite(id: string, address: string) {
  delete instances.errors[id];
  instances.games[id] = { id, phase: "preparing", stage: null, progress: null, error: null };
  try {
    await invoke("join_invite", { id, address });
  } catch (e) {
    delete instances.games[id];
    instances.errors[id] = String(e);
    throw e;
  }
}

export function answerJoinRequest(request: JoinRequest, accept: boolean) {
  invites.requests = invites.requests.filter((r) => r.userId !== request.userId);
  return invoke("answer_join_request", { userId: request.userId, accept });
}

export const fitLabels: Record<Fit, string> = {
  samePackVersion: "Same modpack version",
  samePack: "Same modpack, different version",
  sameVersionAndLoader: "Same version and loader",
  sameVersion: "Same version, different loader",
};
