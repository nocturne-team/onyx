import { invoke, isTauri } from "@tauri-apps/api/core";
import type { CardProject } from "./components/ProjectCard.svelte";

export type ProjectType = "mod" | "modpack" | "resourcepack" | "shader";
export type SearchIndex = "relevance" | "downloads" | "follows" | "newest" | "updated";

/** How stale a cached response may be: none, ~5 minutes, or ~1 hour (kept on disk). */
export type CacheMode = "none" | "short" | "long";

/** Mirrors `onyx_core::modrinth::SearchParams`. */
export interface SearchParams {
  query?: string;
  projectType?: ProjectType;
  loaders?: string[];
  gameVersions?: string[];
  index?: SearchIndex;
  offset?: number;
  limit?: number;
}

/** Mirrors `onyx_core::modrinth::SearchHit`. */
export interface SearchHit {
  projectId: string;
  slug: string | null;
  title: string;
  description: string;
  projectType: string;
  author: string;
  iconUrl: string | null;
  color: number | null;
  downloads: number;
  follows: number;
  dateModified: string;
  displayCategories: string[];
  versions: string[];
  featuredGallery: string | null;
  gallery: string[];
}

export interface SearchResults {
  hits: SearchHit[];
  offset: number;
  limit: number;
  totalHits: number;
}

export async function search(params: SearchParams, cache: CacheMode): Promise<SearchResults> {
  if (!isTauri()) throw new Error("Modrinth calls only work inside the Onyx window");
  return invoke<SearchResults>("search", { params, cache });
}

export function toCardProject(hit: SearchHit): CardProject {
  return {
    title: hit.title,
    author: hit.author,
    description: hit.description,
    iconUrl: hit.iconUrl ?? undefined,
    bannerUrl: hit.featuredGallery ?? hit.gallery[0] ?? undefined,
    color: hit.color != null ? `#${hit.color.toString(16).padStart(6, "0")}` : undefined,
    downloads: hit.downloads,
    follows: hit.follows,
    updated: new Date(hit.dateModified),
  };
}
