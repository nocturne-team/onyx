import type { Page } from "./nav";

/** Which page is showing. Module-level so any component can navigate. */
export const nav = $state({
  page: "home" as Page,
  /** The instance shown when `page` is "instance". */
  instanceId: null as string | null,
});

export function openInstance(id: string) {
  nav.instanceId = id;
  nav.page = "instance";
}
