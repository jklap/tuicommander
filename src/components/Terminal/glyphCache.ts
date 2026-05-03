import { measureFont, type CellMetrics } from "./canvasTerminalUtils";

interface CacheConfig {
  fontSize: number;
  fontFamily: string;
  fontWeight: number;
  dpr: number;
  lineHeight: number;
}

interface GlyphEntry {
  x: number;
  y: number;
  w: number;
  h: number;
}

const ATLAS_SIZE = 2048;

let config: CacheConfig | null = null;
let sharedMetrics: CellMetrics | null = null;
let atlas: OffscreenCanvas | null = null;
let atlasCtx: OffscreenCanvasRenderingContext2D | null = null;
let glyphs = new Map<string, GlyphEntry>();
let nextX = 0;
let nextY = 0;
let rowHeight = 0;
let refCount = 0;

function configMatches(a: CacheConfig, b: CacheConfig): boolean {
  return a.fontSize === b.fontSize
    && a.fontFamily === b.fontFamily
    && a.fontWeight === b.fontWeight
    && a.dpr === b.dpr
    && a.lineHeight === b.lineHeight;
}

function ensureAtlas(): void {
  if (atlas) return;
  atlas = new OffscreenCanvas(ATLAS_SIZE, ATLAS_SIZE);
  atlasCtx = atlas.getContext("2d", { willReadFrequently: false })!;
}

function resetAtlas(): void {
  glyphs.clear();
  nextX = 0;
  nextY = 0;
  if (atlasCtx && atlas) {
    atlasCtx.clearRect(0, 0, atlas.width, atlas.height);
  }
}

function invalidate(): void {
  config = null;
  sharedMetrics = null;
  resetAtlas();
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

  ensureAtlas();
  sharedMetrics = measureFont(atlasCtx!, fontSize, fontFamily, dpr, lineHeight, fontWeight);
  rowHeight = sharedMetrics.scaledCellHeight;
  return sharedMetrics;
}

function rasterize(
  char: string,
  fontStyle: string,
  fgColor: string,
  m: CellMetrics,
): GlyphEntry | null {
  if (!atlasCtx || !atlas) return null;

  const w = m.scaledCellWidth;
  const h = m.scaledCellHeight;

  if (nextX + w > atlas.width) {
    nextX = 0;
    nextY += rowHeight;
  }
  if (nextY + h > atlas.height) {
    resetAtlas();
  }

  const x = nextX;
  const y = nextY;

  atlasCtx.save();
  atlasCtx.font = fontStyle;
  atlasCtx.fillStyle = fgColor;
  atlasCtx.textBaseline = "alphabetic";
  atlasCtx.fillText(char, x, y + m.baseline * m.dpr);
  atlasCtx.restore();

  nextX += w;
  return { x, y, w, h };
}

/**
 * Draw a glyph from the shared atlas cache.
 * Returns false if the cache cannot serve this request (caller should fallback to fillText).
 */
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
    const scaledFont = fontStyle.replace(
      `${m.fontSize}px`,
      `${m.fontSize * m.dpr}px`,
    );
    entry = rasterize(char, scaledFont, fgColor, m) ?? undefined;
    if (!entry) return false;
    glyphs.set(key, entry);
  }

  ctx.drawImage(
    atlas,
    entry.x, entry.y, entry.w, entry.h,
    dx, dy, m.cellWidth, m.cellHeight,
  );
  return true;
}

/**
 * Notify the cache that a terminal mounted.
 * Keeps the atlas alive while any terminal exists.
 */
export function acquireCache(): void {
  refCount++;
}

/**
 * Notify the cache that a terminal unmounted.
 * When the last terminal unmounts, release atlas memory.
 */
export function releaseCache(): void {
  refCount = Math.max(0, refCount - 1);
  if (refCount === 0) {
    atlas = null;
    atlasCtx = null;
    invalidate();
  }
}

/**
 * Force-invalidate on settings change (font family, size, weight, theme).
 */
export function invalidateGlyphCache(): void {
  invalidate();
}
