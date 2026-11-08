export type LibraryTab = "instances" | "projects";

/** Module-level so other places (the account menu) can open a specific tab. */
export const library = $state({ tab: "instances" as LibraryTab });
