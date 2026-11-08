import { search, type ProjectType, type SearchHit, type SearchIndex } from "../modrinth";

const PAGE_SIZE = 20;

/** Lives at module level so the query and results survive switching pages. */
export const browse = $state({
  query: "",
  projectType: "mod" as ProjectType,
  index: "relevance" as SearchIndex,
  hits: [] as SearchHit[],
  totalHits: 0,
  loading: false,
  error: null as string | null,
  /**
   * When set, Browse is filtered to what fits this instance and Install
   * buttons install straight into it.
   */
  target: null as { id: string; name: string; loader: string; gameVersion: string } | null,
});

/** Loader and game version facets for the target instance, if any. */
function targetFilters() {
  const target = browse.target;
  if (!target) return {};
  const loaders =
    browse.projectType === "mod"
      ? target.loader === "quilt"
        ? ["quilt", "fabric"]
        : [target.loader]
      : undefined;
  return { loaders, gameVersions: [target.gameVersion] };
}

// Bumped on every new search so responses from older ones get dropped.
let generation = 0;

export async function runSearch() {
  const current = ++generation;
  browse.loading = true;
  browse.error = null;
  try {
    const results = await search(
      {
        query: browse.query,
        projectType: browse.projectType,
        index: browse.index,
        limit: PAGE_SIZE,
        ...targetFilters(),
      },
      "short",
    );
    if (current !== generation) return;
    browse.hits = results.hits;
    browse.totalHits = results.totalHits;
  } catch (e) {
    if (current !== generation) return;
    browse.hits = [];
    browse.totalHits = 0;
    browse.error = String(e);
  } finally {
    if (current === generation) browse.loading = false;
  }
}

export async function loadMore() {
  if (browse.loading || browse.hits.length >= browse.totalHits) return;
  const current = generation;
  browse.loading = true;
  try {
    const results = await search(
      {
        query: browse.query,
        projectType: browse.projectType,
        index: browse.index,
        offset: browse.hits.length,
        limit: PAGE_SIZE,
        ...targetFilters(),
      },
      "short",
    );
    if (current !== generation) return;
    const seen = new Set(browse.hits.map((h) => h.projectId));
    browse.hits.push(...results.hits.filter((h) => !seen.has(h.projectId)));
    browse.totalHits = results.totalHits;
  } catch (e) {
    if (current === generation) browse.error = String(e);
  } finally {
    if (current === generation) browse.loading = false;
  }
}
