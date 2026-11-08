/**
 * Built-in instance icons: a background gradient with a symbol on top.
 *
 * The symbol images and gradient colors come from Modrinth's app
 * (github.com/modrinth/code, packages/assets/illustrations/instance-icons and
 * packages/ui/src/components/base/icon-editor-modal/editor-catalog.ts),
 * GPL-3.0-only, © Rinth, Inc. See ui/src/assets/instance-icons/NOTICE.md.
 */

export interface IconConfig {
  background: string;
  symbol: string;
}

export interface Background {
  id: string;
  name: string;
  top: string;
  bottom: string;
}

export const backgrounds: Background[] = [
  { id: "rose", name: "Rose", top: "#D62E63", bottom: "#F95C62" },
  { id: "orange", name: "Orange", top: "#FF8D29", bottom: "#FFB452" },
  { id: "yellow", name: "Yellow", top: "#FFC629", bottom: "#FFEE53" },
  { id: "lime", name: "Lime", top: "#6FDA1D", bottom: "#CBFF50" },
  { id: "green", name: "Green", top: "#0B9F21", bottom: "#4FD24B" },
  { id: "purple", name: "Purple", top: "#4739FF", bottom: "#6670FF" },
  { id: "blue", name: "Blue", top: "#227EFF", bottom: "#5EC1FF" },
  { id: "lavender", name: "Lavender", top: "#C056FD", bottom: "#B889FF" },
  { id: "pink", name: "Pink", top: "#F640C0", bottom: "#FF7BF1" },
  { id: "light-gray", name: "Light gray", top: "#AEAEAE", bottom: "#D9D9D9" },
  { id: "gray", name: "Gray", top: "#373C4C", bottom: "#4C4F58" },
  { id: "dark-gray", name: "Dark gray", top: "#1B1D29", bottom: "#252731" },
];

export type SymbolCategory = "loader" | "modded" | "vanilla";

export interface IconSymbol {
  id: string;
  name: string;
  url: string;
  category: SymbolCategory;
}

const files = import.meta.glob<string>("../assets/instance-icons/*.png", {
  eager: true,
  query: "?url",
  import: "default",
});

const loaders = ["fabric", "forge", "neoforge", "quilt"];
const modded = [
  "poke-ball", "orb", "cooking-pot", "skillet", "globe", "pancakes", "backpack", "couch",
  "tiny-potato", "blue-shark", "brown-bear", "moobloom", "wrench", "cogwheel", "engine", "tire",
  "oxygen-distributor", "space-helmet", "gizmo", "terminal",
];
const names: Record<string, string> = { neoforge: "NeoForge", tnt: "TNT", wrench: "Wrench" };

const categoryOrder: SymbolCategory[] = ["loader", "modded", "vanilla"];

export const symbols: IconSymbol[] = Object.entries(files)
  .map(([path, url]) => {
    const id = path.split("/").pop()!.replace(/\.png$/, "");
    const category: SymbolCategory = loaders.includes(id)
      ? "loader"
      : modded.includes(id)
        ? "modded"
        : "vanilla";
    const name =
      names[id] ?? id.replace(/-/g, " ").replace(/^./, (c) => c.toUpperCase());
    return { id, name, url, category };
  })
  .sort(
    (a, b) =>
      categoryOrder.indexOf(a.category) - categoryOrder.indexOf(b.category) ||
      (a.category === "modded" ? modded.indexOf(a.id) - modded.indexOf(b.id) : a.name.localeCompare(b.name)),
  );

export const categoryNames: Record<SymbolCategory, string> = {
  loader: "Loaders",
  modded: "Modded",
  vanilla: "Vanilla",
};

export function backgroundCss(id: string): string {
  const bg = backgrounds.find((b) => b.id === id) ?? backgrounds[0];
  return `linear-gradient(to bottom, ${bg.top}, ${bg.bottom})`;
}

export function symbolUrl(id: string): string | undefined {
  return symbols.find((s) => s.id === id)?.url;
}

/** Loader logos are for picking on purpose, not for random icons. */
const randomSymbols = () => symbols.filter((s) => s.category !== "loader");

export function randomIconConfig(): IconConfig {
  const pick = <T>(list: T[]) => list[Math.floor(Math.random() * list.length)];
  return { background: pick(backgrounds).id, symbol: pick(randomSymbols()).id };
}

/** FNV-1a, so instances without a saved icon still get a stable one. */
function hash(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

export function defaultIconConfig(instanceId: string): IconConfig {
  const h = hash(instanceId);
  const pool = randomSymbols();
  return {
    background: backgrounds[h % backgrounds.length].id,
    symbol: pool[Math.floor(h / backgrounds.length) % pool.length].id,
  };
}
