import type { Page } from "./nav";
import { browse, runSearch } from "./pages/browseState.svelte";

/** Which page is showing. Module-level so any component can navigate. */
export const nav = $state({
  page: "home" as Page,
  /** The instance shown when `page` is "instance". */
  instanceId: null as string | null,
  /** The Modrinth project shown when `page` is "project". */
  projectId: null as string | null,
  /** Title and icon of the open project, set by the project page once loaded. */
  project: null as { title: string; iconUrl: string | null } | null,
  /** Where the project page's back button goes. */
  back: null as { page: Page; instanceId: string | null } | null,
});

export function openInstance(id: string) {
  nav.instanceId = id;
  nav.page = "instance";
}

export function openProject(id: string) {
  if (nav.page !== "project") nav.back = { page: nav.page, instanceId: nav.instanceId };
  if (nav.projectId !== id) nav.project = null;
  nav.projectId = id;
  nav.page = "project";
}

export function goBack() {
  const back = nav.back ?? { page: "browse" as Page, instanceId: null };
  nav.instanceId = back.instanceId;
  nav.page = back.page;
  nav.back = null;
}

// Back/forward history for the titlebar arrows. Every distinct place the user
// lands on is recorded by `recordNavigation`, which App runs in an effect.

interface Entry {
  page: Page;
  instanceId: string | null;
  projectId: string | null;
  back: typeof nav.back;
  target: typeof browse.target;
}

const MAX_ENTRIES = 50;

export const history = $state({ entries: [] as Entry[], index: -1 });

// Set while we're applying an entry, so the effect doesn't record it again.
let restoring = false;

function current(): Entry {
  return {
    page: nav.page,
    instanceId: nav.instanceId,
    projectId: nav.projectId,
    back: nav.back,
    target: browse.target,
  };
}

function samePlace(a: Entry, b: Entry) {
  if (a.page !== b.page) return false;
  if (a.page === "instance") return a.instanceId === b.instanceId;
  if (a.page === "project") return a.projectId === b.projectId;
  if (a.page === "browse") return a.target?.id === b.target?.id;
  return true;
}

/** Call from an effect: reads the tracked nav state and records a new entry if it moved. */
export function recordNavigation() {
  const entry = current();
  if (restoring) {
    restoring = false;
    return;
  }
  const here = history.entries[history.index];
  if (here && samePlace(here, entry)) return;
  const entries = history.entries.slice(0, history.index + 1);
  entries.push(entry);
  if (entries.length > MAX_ENTRIES) entries.shift();
  history.entries = entries;
  history.index = entries.length - 1;
}

function restore(index: number) {
  const entry = history.entries[index];
  if (!entry) return;
  history.index = index;
  restoring = true;
  const targetChanged = browse.target?.id !== entry.target?.id;
  browse.target = entry.target;
  if (targetChanged) runSearch();
  if (nav.projectId !== entry.projectId) nav.project = null;
  nav.instanceId = entry.instanceId;
  nav.projectId = entry.projectId;
  nav.back = entry.back;
  nav.page = entry.page;
}

export const canGoBack = () => history.index > 0;
export const canGoForward = () => history.index < history.entries.length - 1;
export const historyBack = () => canGoBack() && restore(history.index - 1);
export const historyForward = () => canGoForward() && restore(history.index + 1);
