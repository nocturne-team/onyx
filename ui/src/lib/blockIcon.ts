/**
 * Generated instance icons: an 8x8 pixel "block" with a random hue, noise and
 * a light top-left / dark bottom-right bevel. Deterministic per seed.
 */

/** FNV-1a, for turning an instance id into a seed. */
export function seedFromString(text: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

export function randomSeed(): number {
  return crypto.getRandomValues(new Uint32Array(1))[0];
}

/** mulberry32: small, fast, good enough for pixel noise. */
function prng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** 64 HSL colors, row by row. */
export function blockPixels(seed: number): string[] {
  const random = prng(seed);
  const hue = Math.floor(random() * 360);
  const saturation = 30 + Math.floor(random() * 35);
  const base = 38 + Math.floor(random() * 14);
  // A second, nearby hue for speckles, like ore in stone.
  const accent = (hue + 20 + Math.floor(random() * 40)) % 360;
  const pixels: string[] = [];
  for (let y = 0; y < 8; y++) {
    for (let x = 0; x < 8; x++) {
      let lightness = base + (random() - 0.5) * 14;
      if (x === 0 || y === 0) lightness += 9;
      if (x === 7 || y === 7) lightness -= 11;
      const speckle = random() < 0.12;
      pixels.push(
        `hsl(${speckle ? accent : hue} ${saturation + (speckle ? 15 : 0)}% ${Math.round(lightness + (speckle ? 10 : 0))}%)`,
      );
    }
  }
  return pixels;
}
