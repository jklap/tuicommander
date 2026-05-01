// --- Binary frame decoding and font measurement for CanvasTerminal ---

// Wire format constants (must match terminal_grid.rs)
const HEADER_SIZE = 7;
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

  return { cursorRow, cursorCol, cursorVisible, rows };
}

/**
 * Measure a monospace font and return cell metrics for grid layout.
 * Uses 'M' as the reference glyph (widest ASCII char in most monospace fonts).
 */
export function measureFont(
  ctx: CanvasRenderingContext2D,
  fontSize: number,
  fontFamily: string,
  dpr: number = 1,
): CellMetrics {
  ctx.font = `${fontSize}px ${fontFamily}`;
  const m = ctx.measureText("M");
  const cellWidth = Math.ceil(m.width);
  const ascent = Math.ceil(m.actualBoundingBoxAscent);
  const descent = Math.ceil(m.actualBoundingBoxDescent);
  const cellHeight = ascent + descent + 1;

  return {
    cellWidth,
    cellHeight,
    baseline: ascent,
    fontSize,
    dpr,
    scaledCellWidth: cellWidth * dpr,
    scaledCellHeight: cellHeight * dpr,
  };
}
