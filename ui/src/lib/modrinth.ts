import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
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
  /** All of these must match. */
  categories?: string[];
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

/** Mirrors `onyx_core::modrinth::ProjectPage`. */
export interface ProjectPage {
  id: string;
  slug: string | null;
  title: string;
  description: string;
  /** Untrusted Markdown; render only through `renderMarkdown`. */
  body: string;
  projectType: string;
  iconUrl: string | null;
  color: number | null;
  downloads: number;
  followers: number;
  categories: string[];
  loaders: string[];
  gameVersions: string[];
  published: string;
  updated: string;
  license: { id: string; name: string; url: string | null } | null;
  sourceUrl: string | null;
  issuesUrl: string | null;
  wikiUrl: string | null;
  discordUrl: string | null;
  donationUrls: { platform: string; url: string }[];
  gallery: { url: string; featured: boolean; title: string | null; description: string | null }[];
  author: string | null;
}

/** Mirrors `onyx_core::modrinth::Version` (the fields shown). */
export interface ProjectVersion {
  id: string;
  name: string;
  versionNumber: string;
  versionType: "release" | "beta" | "alpha";
  gameVersions: string[];
  loaders: string[];
  datePublished: string;
  files: { filename: string; size: number; primary: boolean }[];
}

export function projectPage(id: string): Promise<ProjectPage> {
  return invoke<ProjectPage>("project_page", { id });
}

export function projectVersions(id: string): Promise<ProjectVersion[]> {
  return invoke<ProjectVersion[]>("project_versions", { id });
}

/** What the Install button needs from a project, shaped like a search hit. */
export function pageAsHit(page: ProjectPage): SearchHit {
  return {
    projectId: page.id,
    slug: page.slug,
    title: page.title,
    description: page.description,
    projectType: page.projectType,
    author: page.author ?? "",
    iconUrl: page.iconUrl,
    color: page.color,
    downloads: page.downloads,
    follows: page.followers,
    dateModified: page.updated,
    displayCategories: page.categories,
    versions: page.gameVersions,
    featuredGallery: null,
    gallery: [],
  };
}

/** Mirrors `onyx_core::modrinth::Category`. `icon` is untrusted SVG markup. */
export interface Category {
  name: string;
  icon: string;
  projectType: string;
  header: string;
}

let categoryList: Promise<Category[]> | null = null;

/** All category tags, fetched once per session (and cached on disk for an hour). */
export function categories(): Promise<Category[]> {
  if (!isTauri()) return Promise.resolve([]);
  categoryList ??= invoke<Category[]>("categories").catch((e) => {
    categoryList = null;
    throw e;
  });
  return categoryList;
}

/**
 * A Modrinth CDN image through Onyx's disk cache (the `onyx-icon` scheme),
 * so it shows without a network round trip after the first time.
 */
export function cachedIcon(url: string): string {
  if (!isTauri() || !url.startsWith("https://cdn.modrinth.com/")) return url;
  return convertFileSrc(url, "onyx-icon");
}
