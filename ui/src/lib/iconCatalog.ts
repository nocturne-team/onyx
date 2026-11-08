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

/**
 * Pride flags as icon backgrounds. Picked on purpose in the icon editor, so
 * they are not in `backgrounds` and never come out of Randomize or the
 * hash-based default. Stripes are equal height; repeat a color to weight it.
 */
export interface PrideFlag {
  id: string;
  name: string;
  stripes: string[];
}

export const prideFlags: PrideFlag[] = [
  { id: "pride-rainbow", name: "Rainbow", stripes: ["#E40303", "#FF8C00", "#FFED00", "#008026", "#004DFF", "#750787"] },
  { id: "pride-trans", name: "Transgender", stripes: ["#5BCEFA", "#F5A9B8", "#FFFFFF", "#F5A9B8", "#5BCEFA"] },
  { id: "pride-bi", name: "Bisexual", stripes: ["#D60270", "#D60270", "#9B4F96", "#0038A8", "#0038A8"] },
  { id: "pride-pan", name: "Pansexual", stripes: ["#FF218C", "#FFD800", "#21B1FF"] },
  { id: "pride-lesbian", name: "Lesbian", stripes: ["#D52D00", "#EF7627", "#FF9A56", "#FFFFFF", "#D162A4", "#B55690", "#A30262"] },
  { id: "pride-ace", name: "Asexual", stripes: ["#000000", "#A3A3A3", "#FFFFFF", "#800080"] },
  { id: "pride-aro", name: "Aromantic", stripes: ["#3DA542", "#A7D379", "#FFFFFF", "#A9A9A9", "#000000"] },
  { id: "pride-nonbinary", name: "Non-binary", stripes: ["#FCF434", "#FFFFFF", "#9C59D1", "#2C2C2C"] },
  { id: "pride-demigirl", name: "Demigirl", stripes: ["#808080", "#C4C4C4", "#FDADC8", "#FFFFFF", "#FDADC8", "#C4C4C4", "#808080"] },
  { id: "pride-demiboy", name: "Demiboy", stripes: ["#808080", "#C4C4C4", "#9ADAEB", "#FFFFFF", "#9ADAEB", "#C4C4C4", "#808080"] },
  { id: "pride-genderfluid", name: "Genderfluid", stripes: ["#FF76A4", "#FFFFFF", "#C011D7", "#000000", "#2F3CBE"] },
  { id: "pride-genderqueer", name: "Genderqueer", stripes: ["#B57EDC", "#FFFFFF", "#4A8123"] },
  { id: "pride-agender", name: "Agender", stripes: ["#000000", "#BABABA", "#FFFFFF", "#B7F684", "#FFFFFF", "#BABABA", "#000000"] },
  { id: "pride-bigender", name: "Bigender", stripes: ["#C479A2", "#EDA5CD", "#D6C7E8", "#FFFFFF", "#D6C7E8", "#9AC7E8", "#6D82D1"] },
  { id: "pride-gay", name: "Gay men", stripes: ["#078D70", "#26CEAA", "#98E8C1", "#FFFFFF", "#7BADE2", "#5049CC", "#3D1A78"] },
  { id: "pride-polysexual", name: "Polysexual", stripes: ["#F714BA", "#01D66A", "#1594F6"] },
  { id: "pride-omnisexual", name: "Omnisexual", stripes: ["#FE9ACE", "#FF53BF", "#200044", "#6760FE", "#8EA6FF"] },
  { id: "pride-aroace", name: "Aroace", stripes: ["#E28C00", "#ECCD00", "#FFFFFF", "#62AEDC", "#203856"] },
  { id: "pride-graysexual", name: "Graysexual", stripes: ["#740194", "#AEB1B2", "#FFFFFF", "#AEB1B2", "#740194"] },
  { id: "pride-abrosexual", name: "Abrosexual", stripes: ["#46D294", "#A2E9C8", "#FFFFFF", "#F78BB3", "#EB4E8A"] },
  { id: "pride-androgyne", name: "Androgyne", stripes: ["#FE007F", "#9832FF", "#00B8E7"] },
  { id: "pride-pangender", name: "Pangender", stripes: ["#FFF798", "#FFDDCD", "#FFEBFB", "#FFFFFF", "#FFEBFB", "#FFDDCD", "#FFF798"] },
  { id: "pride-neutrois", name: "Neutrois", stripes: ["#FFFFFF", "#1F9F00", "#000000"] },
  { id: "pride-polyamory", name: "Polyamory", stripes: ["#009FE3", "#E50000", "#000000"] },
];

function stripesCss(colors: string[]): string {
  const step = 100 / colors.length;
  const parts = colors.map((c, i) => `${c} ${+(i * step).toFixed(3)}% ${+((i + 1) * step).toFixed(3)}%`);
  return `linear-gradient(to bottom, ${parts.join(", ")})`;
}

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
  const flag = prideFlags.find((f) => f.id === id);
  if (flag) return stripesCss(flag.stripes);
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
