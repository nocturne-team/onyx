export type Page = "home" | "browse" | "library" | "instance" | "project";

export const pageTitles: Record<Exclude<Page, "instance" | "project">, string> = {
  home: "Home",
  browse: "Browse",
  library: "Library",
};
