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
const GLYPH_PAD = 2;

let config: CacheConfig | null = null;
let sharedMetrics: CellMetrics | null = null;
let atlas: HTMLCanvasElement | null = null;
let atlasCtx: CanvasRenderingContext2D | null = null;
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
  atlas = document.createElement("canvas");
  atlas.width = ATLAS_SIZE;
  atlas.height = ATLAS_SIZE;
  atlas.style.display = "none";
  document.body.appendChild(atlas);
  atlasCtx = atlas.getContext("2d", { alpha: true })!;
}

function resetGlyphs(): void {
  glyphs.clear();
  nextX = 0;
  nextY = 0;
  if (atlasCtx && atlas) {
    atlasCtx.clearRect(0, 0, atlas.width, atlas.height);
  }
}

function destroyAtlas(): void {
  if (atlas?.parentElement) {
    atlas.parentElement.removeChild(atlas);
  }
  atlas = null;
  atlasCtx = null;
  resetGlyphs();
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
  ensureAtlas();
  sharedMetrics = measureFont(atlasCtx!, fontSize, fontFamily, dpr, lineHeight, fontWeight);
  rowHeight = sharedMetrics.scaledCellHeight + GLYPH_PAD;
  return sharedMetrics;
}

function rasterize(
  char: string,
  scaledFont: string,
  fgColor: string,
  m: CellMetrics,
): GlyphEntry | null {
  if (!atlasCtx || !atlas) return null;

  const w = m.scaledCellWidth;
  const h = m.scaledCellHeight;
  const slot = w + GLYPH_PAD;

  if (nextX + slot > atlas.width) {
    nextX = 0;
    nextY += rowHeight;
  }
  if (nextY + h > atlas.height) {
    resetGlyphs();
  }

  const x = nextX;
  const y = nextY;

  atlasCtx.clearRect(x, y, slot, h);
  atlasCtx.font = scaledFont;
  atlasCtx.fillStyle = fgColor;
  atlasCtx.textBaseline = "alphabetic";
  atlasCtx.fillText(char, x, y + m.baseline * m.dpr);

  nextX += slot;
  return { x, y, w, h };
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
    entry.x, entry.y, entry.w + GLYPH_PAD, entry.h,
    dx, dy, m.cellWidth + GLYPH_PAD / m.dpr, m.cellHeight,
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
    invalidate();
  }
}

export function invalidateGlyphCache(): void {
  invalidate();
}
