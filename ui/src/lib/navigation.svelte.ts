import type { Page } from "./nav";

/** Which page is showing. Module-level so any component can navigate. */
export const nav = $state({
  page: "home" as Page,
  /** The instance shown when `page` is "instance". */
  instanceId: null as string | null,
  /** The Modrinth project shown when `page` is "project". */
  projectId: null as string | null,
  /** Where the project page's back button goes. */
  back: null as { page: Page; instanceId: string | null } | null,
});

export function openInstance(id: string) {
  nav.instanceId = id;
  nav.page = "instance";
}

export function openProject(id: string) {
  if (nav.page !== "project") nav.back = { page: nav.page, instanceId: nav.instanceId };
  nav.projectId = id;
  nav.page = "project";
}

export function goBack() {
  const back = nav.back ?? { page: "browse" as Page, instanceId: null };
  nav.instanceId = back.instanceId;
  nav.page = back.page;
  nav.back = null;
}
