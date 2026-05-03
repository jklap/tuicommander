// --- Binary frame decoding and font measurement for CanvasTerminal ---

// Wire format constants (must match terminal_grid.rs)
const HEADER_SIZE = 22;
const CELL_SIZE = 11; // 4 (char u32) + 3 (fg) + 3 (bg) + 1 (attrs)
const ATTR_BOLD = 0x01;
const ATTR_ITALIC = 0x02;
const ATTR_UNDERLINE = 0x04;
const ATTR_STRIKEOUT = 0x08;
const ATTR_DIM = 0x10;
const ATTR_INVERSE = 0x20;
const ATTR_DEFAULT_FG = 0x40;
const ATTR_DEFAULT_BG = 0x80;

export interface DecodedCell {
  char: string;
  fgR: number;
  fgG: number;
  fgB: number;
  bgR: number;
  bgG: number;
  bgB: number;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strikeout: boolean;
  dim: boolean;
  inverse: boolean;
  defaultFg: boolean;
  defaultBg: boolean;
}

export interface DecodedRow {
  index: number;
  cells: DecodedCell[];
}

export interface DecodedFrame {
  cursorRow: number;
  cursorCol: number;
  cursorVisible: boolean;
  cursorShape: "block" | "underline" | "beam";
  displayOffset: number;
  historySize: number;
  hasSelection: boolean;
  keyboardFlags: number;
  bell: boolean;
  screenRows: number;
  screenCols: number;
  rows: DecodedRow[];
}

export interface CellMetrics {
  cellWidth: number;
  cellHeight: number;
  baseline: number;
  fontSize: number;
  dpr: number;
  scaledCellWidth: number;
  scaledCellHeight: number;
}

/** Decode a binary grid frame from the Rust backend into structured data. */
export function decodeBinaryFrame(buffer: ArrayBuffer): DecodedFrame | null {
  if (buffer.byteLength < HEADER_SIZE) return null;

  const view = new DataView(buffer);
  let offset = 0;

  const numRows = view.getUint16(offset, true); offset += 2;
  const cursorRow = view.getUint16(offset, true); offset += 2;
  const cursorCol = view.getUint16(offset, true); offset += 2;
  const cursorVisible = view.getUint8(offset) !== 0; offset += 1;
  const displayOffset = view.getUint32(offset, true); offset += 4;
  const historySize = view.getUint32(offset, true); offset += 4;
  const hasSelection = view.getUint8(offset) !== 0; offset += 1;
  const keyboardFlags = view.getUint8(offset); offset += 1;
  const frameFlags = view.getUint8(offset); offset += 1;
  const screenRows = view.getUint16(offset, true); offset += 2;
  const screenCols = view.getUint16(offset, true); offset += 2;
  const bell = (frameFlags & 0x01) !== 0;
  const cursorShapeRaw = (frameFlags >> 1) & 0x03;
  const cursorShape: "block" | "underline" | "beam" = cursorShapeRaw === 2 ? "beam" : cursorShapeRaw === 1 ? "underline" : "block";

  const rows: DecodedRow[] = [];
  for (let r = 0; r < numRows; r++) {
    if (offset + 4 > buffer.byteLength) break;
    const rowIndex = view.getUint16(offset, true); offset += 2;
    const colCount = view.getUint16(offset, true); offset += 2;

    const cells: DecodedCell[] = [];
    for (let c = 0; c < colCount; c++) {
      if (offset + CELL_SIZE > buffer.byteLength) break;
      const cp = view.getUint32(offset, true); offset += 4;
      const fgR = view.getUint8(offset); offset += 1;
      const fgG = view.getUint8(offset); offset += 1;
      const fgB = view.getUint8(offset); offset += 1;
      const bgR = view.getUint8(offset); offset += 1;
      const bgG = view.getUint8(offset); offset += 1;
      const bgB = view.getUint8(offset); offset += 1;
      const attrs = view.getUint8(offset); offset += 1;

      cells.push({
        char: cp === 0 ? "" : String.fromCodePoint(cp),
        fgR, fgG, fgB,
        bgR, bgG, bgB,
        bold: (attrs & ATTR_BOLD) !== 0,
        italic: (attrs & ATTR_ITALIC) !== 0,
        underline: (attrs & ATTR_UNDERLINE) !== 0,
        strikeout: (attrs & ATTR_STRIKEOUT) !== 0,
        dim: (attrs & ATTR_DIM) !== 0,
        inverse: (attrs & ATTR_INVERSE) !== 0,
        defaultFg: (attrs & ATTR_DEFAULT_FG) !== 0,
        defaultBg: (attrs & ATTR_DEFAULT_BG) !== 0,
      });
    }

    rows.push({ index: rowIndex, cells });
  }

  return { cursorRow, cursorCol, cursorVisible, cursorShape, displayOffset, historySize, hasSelection, keyboardFlags, bell, screenRows, screenCols, rows };
}

/** Measure natural character height via DOM span — matches xterm.js CharSizeService. */
export function measureCharHeightDOM(fontSize: number, fontFamily: string, fontWeight: number = 400): number {
  const span = document.createElement("span");
  span.style.font = `${fontWeight} ${fontSize}px ${fontFamily}`;
  span.style.lineHeight = "normal";
  span.style.position = "absolute";
  span.style.visibility = "hidden";
  span.textContent = "W";
  document.body.appendChild(span);
  const h = span.getBoundingClientRect().height;
  document.body.removeChild(span);
  return h;
}

/** Snap lineHeight to integer device pixels to prevent sub-pixel seams between rows. */
export function snapLineHeight(fontSize: number, target: number = 1.2): number {
  const dpr = window.devicePixelRatio || 1;
  const rawDevicePx = fontSize * target * dpr;
  const lo = Math.floor(rawDevicePx);
  const hi = Math.ceil(rawDevicePx);
  const best = (Math.abs(rawDevicePx - lo) <= Math.abs(rawDevicePx - hi)) ? lo : hi;
  const snapped = best / (fontSize * dpr);
  return Math.max(1.0, Math.min(snapped, 1.5));
}

export type CursorShape = "block" | "beam" | "underline";

export interface CursorRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Compute the pixel rectangle for a cursor at the given grid position. */
export function computeCursorRect(
  shape: CursorShape,
  row: number,
  col: number,
  m: CellMetrics,
): CursorRect {
  const x = col * m.cellWidth;
  const y = row * m.cellHeight;
  switch (shape) {
    case "block":
      return { x, y, w: m.cellWidth, h: m.cellHeight };
    case "beam":
      return { x, y, w: 2, h: m.cellHeight };
    case "underline":
      return { x, y: y + m.cellHeight - 2, w: m.cellWidth, h: 2 };
  }
}

/**
 * Measure a monospace font and return cell metrics for grid layout.
 * Matches xterm.js WebGL renderer dimension calculation exactly:
 *   device.char.height = ceil(charHeight * dpr)
 *   device.cell.height = floor(device.char.height * lineHeight)
 *   charTop = round((cellHeight_device - charHeight_device) / 2)
 */
export function measureFont(
  ctx: CanvasRenderingContext2D,
  fontSize: number,
  fontFamily: string,
  dpr: number = 1,
  lineHeight: number = 1.2,
  fontWeight: number = 400,
  charHeightOverride?: number,
): CellMetrics {
  ctx.font = `${fontWeight} ${fontSize}px ${fontFamily}`;
  const m = ctx.measureText("W");
  const cellWidth = Math.ceil(m.width);

  const ascent = m.fontBoundingBoxAscent ?? m.actualBoundingBoxAscent;
  const descent = m.fontBoundingBoxDescent ?? m.actualBoundingBoxDescent;
  const charHeightCSS = charHeightOverride ?? (ascent + descent);

  // xterm.js WebGL formula: compute in device pixels, then convert back
  const charHeightDevice = Math.ceil(charHeightCSS * dpr);
  const cellHeightDevice = Math.floor(charHeightDevice * lineHeight);
  const charTopDevice = lineHeight === 1 ? 0 : Math.round((cellHeightDevice - charHeightDevice) / 2);

  const cellHeight = cellHeightDevice / dpr;
  const baseline = Math.ceil(ascent) + charTopDevice / dpr;

  return {
    cellWidth,
    cellHeight,
    baseline: Math.max(baseline, 0),
    fontSize,
    dpr,
    scaledCellWidth: cellWidth * dpr,
    scaledCellHeight: cellHeightDevice,
  };
}
