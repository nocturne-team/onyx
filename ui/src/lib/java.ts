import { invoke, isTauri } from "@tauri-apps/api/core";

/** Mirrors `onyx_core::minecraft::java::JavaInstall`. */
export interface JavaInstall {
  path: string;
  version: string;
  major: number;
}

let detected: Promise<JavaInstall[]> | null = null;

/** Java installs on the system, scanned once per app run (or again on `refresh`). */
export function detectJava(refresh = false): Promise<JavaInstall[]> {
  if (!isTauri()) return Promise.resolve([]);
  if (refresh || !detected) detected = invoke<JavaInstall[]>("detect_java");
  return detected;
}

/** Asks for a Java binary and checks it. `null` if cancelled. */
export async function pickJava(): Promise<JavaInstall | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const path = await open({ multiple: false, directory: false, title: "Pick a java or javaw.exe" });
  if (typeof path !== "string") return null;
  return invoke<JavaInstall>("probe_java", { path });
}
