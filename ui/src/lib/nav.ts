export type Page = "home" | "browse" | "library" | "instance";

export const pageTitles: Record<Exclude<Page, "instance">, string> = {
  home: "Home",
  browse: "Browse",
  library: "Library",
};
