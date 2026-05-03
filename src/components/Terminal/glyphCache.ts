import { measureFont, type CellMetrics } from "./canvasTerminalUtils";

interface CacheConfig {
  fontSize: number;
  fontFamily: string;
  fontWeight: number;
  dpr: number;
  lineHeight: number;
}

interface GlyphEntry {
  cssX: number;
  cssY: number;
}

const ATLAS_SIZE = 2048;
const GLYPH_PAD = 1;

let config: CacheConfig | null = null;
let sharedMetrics: CellMetrics | null = null;
let atlas: HTMLCanvasElement | null = null;
let atlasCtx: CanvasRenderingContext2D | null = null;
let glyphs = new Map<string, GlyphEntry>();
let nextCssX = 0;
let nextCssY = 0;
let refCount = 0;

function configMatches(a: CacheConfig, b: CacheConfig): boolean {
  return a.fontSize === b.fontSize
    && a.fontFamily === b.fontFamily
    && a.fontWeight === b.fontWeight
    && a.dpr === b.dpr
    && a.lineHeight === b.lineHeight;
}

function ensureAtlas(dpr: number): void {
  if (atlas) return;
  atlas = document.createElement("canvas");
  atlas.width = ATLAS_SIZE;
  atlas.height = ATLAS_SIZE;
  atlas.style.display = "none";
  document.body.appendChild(atlas);
  atlasCtx = atlas.getContext("2d", { alpha: true })!;
  atlasCtx.scale(dpr, dpr);
}

function resetGlyphs(): void {
  glyphs.clear();
  nextCssX = 0;
  nextCssY = 0;
  if (atlasCtx && atlas) {
    atlasCtx.setTransform(1, 0, 0, 1, 0, 0);
    atlasCtx.clearRect(0, 0, atlas.width, atlas.height);
    if (config) atlasCtx.scale(config.dpr, config.dpr);
  }
}

function destroyAtlas(): void {
  if (atlas?.parentElement) {
    atlas.parentElement.removeChild(atlas);
  }
  atlas = null;
  atlasCtx = null;
  glyphs.clear();
  nextCssX = 0;
  nextCssY = 0;
}

function invalidate(): void {
  config = null;
  sharedMetrics = null;
  resetGlyphs();
}

export function getSharedMetrics(
  fontSize: number,
  fontFamily: string,
  dpr: number,
  lineHeight: number,
  fontWeight: number,
): CellMetrics {
  const cfg: CacheConfig = { fontSize, fontFamily, fontWeight, dpr, lineHeight };
  if (sharedMetrics && config && configMatches(config, cfg)) {
    return sharedMetrics;
  }

  invalidate();
  config = cfg;
  ensureAtlas(dpr);
  sharedMetrics = measureFont(atlasCtx!, fontSize, fontFamily, dpr, lineHeight, fontWeight);
  return sharedMetrics;
}

function rasterize(
  char: string,
  fontStyle: string,
  fgColor: string,
  m: CellMetrics,
): GlyphEntry | null {
  if (!atlasCtx || !atlas) return null;

  const cssW = m.cellWidth;
  const cssH = m.cellHeight;
  const slot = cssW + GLYPH_PAD;
  const cssAtlasW = atlas.width / m.dpr;
  const cssAtlasH = atlas.height / m.dpr;

  if (nextCssX + slot > cssAtlasW) {
    nextCssX = 0;
    nextCssY += cssH + GLYPH_PAD;
  }
  if (nextCssY + cssH > cssAtlasH) {
    resetGlyphs();
  }

  const cx = nextCssX;
  const cy = nextCssY;

  atlasCtx.clearRect(cx, cy, slot, cssH);
  atlasCtx.font = fontStyle;
  atlasCtx.fillStyle = fgColor;
  atlasCtx.textBaseline = "alphabetic";
  atlasCtx.fillText(char, cx, cy + m.baseline);

  nextCssX += slot;
  return { cssX: cx, cssY: cy };
}

export function drawCachedGlyph(
  ctx: CanvasRenderingContext2D,
  char: string,
  fontStyle: string,
  fgColor: string,
  dx: number,
  dy: number,
  m: CellMetrics,
): boolean {
  if (!atlas || !config) return false;

  const key = `${char}\0${fontStyle}\0${fgColor}`;
  let entry = glyphs.get(key);
  if (!entry) {
    entry = rasterize(char, fontStyle, fgColor, m) ?? undefined;
    if (!entry) return false;
    glyphs.set(key, entry);
  }

  const dpr = m.dpr;
  ctx.drawImage(
    atlas,
    entry.cssX * dpr, entry.cssY * dpr,
    m.scaledCellWidth, m.scaledCellHeight,
    dx, dy,
    m.cellWidth, m.cellHeight,
  );
  return true;
}

export function acquireCache(): void {
  refCount++;
}

export function releaseCache(): void {
  refCount = Math.max(0, refCount - 1);
  if (refCount === 0) {
    destroyAtlas();
    config = null;
    sharedMetrics = null;
  }
}

export function invalidateGlyphCache(): void {
  invalidate();
}
