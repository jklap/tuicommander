import { measureFont, type CellMetrics } from "./canvasTerminalUtils";

interface CacheConfig {
  fontSize: number;
  fontFamily: string;
  fontWeight: number;
  dpr: number;
  lineHeight: number;
}

let config: CacheConfig | null = null;
let sharedMetrics: CellMetrics | null = null;
let measureCtx: OffscreenCanvasRenderingContext2D | null = null;
let refCount = 0;

function configMatches(a: CacheConfig, b: CacheConfig): boolean {
  return a.fontSize === b.fontSize
    && a.fontFamily === b.fontFamily
    && a.fontWeight === b.fontWeight
    && a.dpr === b.dpr
    && a.lineHeight === b.lineHeight;
}

function ensureMeasureCtx(): void {
  if (measureCtx) return;
  const canvas = new OffscreenCanvas(1, 1);
  measureCtx = canvas.getContext("2d")!;
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

  config = cfg;
  ensureMeasureCtx();
  sharedMetrics = measureFont(measureCtx!, fontSize, fontFamily, dpr, lineHeight, fontWeight);
  return sharedMetrics;
}

export function acquireCache(): void {
  refCount++;
}

export function releaseCache(): void {
  refCount = Math.max(0, refCount - 1);
  if (refCount === 0) {
    measureCtx = null;
    config = null;
    sharedMetrics = null;
  }
}

export function invalidateGlyphCache(): void {
  config = null;
  sharedMetrics = null;
}
