// --- Binary frame decoding and font measurement for CanvasTerminal ---

// Layout constants
export const GUTTER_PX = 6;
export const SCROLLBAR_PX = 14;

// Wire format constants (must match terminal_grid.rs)
const HEADER_SIZE = 26;
const CELL_SIZE = 11; // 4 (char u32) + 3 (fg) + 3 (bg) + 1 (attrs)
export const ATTR_BOLD = 0x01;
export const ATTR_ITALIC = 0x02;
export const ATTR_UNDERLINE = 0x04;
export const ATTR_STRIKEOUT = 0x08;
export const ATTR_DIM = 0x10;
export const ATTR_INVERSE = 0x20;
export const ATTR_DEFAULT_FG = 0x40;
export const ATTR_DEFAULT_BG = 0x80;

/** Bit 15 of the wire `col_count`: this row continues onto the next display row.
 *  Mirrors `ROW_WRAPPED_FLAG` in src-tauri/src/terminal_grid.rs — the grid is the
 *  only place that knows a line wrapped, and the overlay needs it to mask a
 *  wrapped `suggest:` block (#8fc7). */
const ROW_WRAPPED_FLAG = 0x8000;

export interface DecodedRow {
	index: number;
	count: number;
	/** True when the line continues onto the next display row. */
	wrapped: boolean;
	/** Unicode codepoints; 0 = empty cell */
	codepoints: Uint32Array;
	/** Packed fg color: r<<16|g<<8|b (valid when ATTR_DEFAULT_FG not set) */
	fg: Uint32Array;
	/** Packed bg color: r<<16|g<<8|b (valid when ATTR_DEFAULT_BG not set) */
	bg: Uint32Array;
	/** Per-cell ATTR_* bitmask */
	attrs: Uint8Array;
}

export interface DecodedFrame {
	cursorRow: number;
	cursorCol: number;
	cursorVisible: boolean;
	/** "default" means the app has not requested a shape via DECSCUSR — the
	 *  caller should fall back to the user's cursor-style setting. */
	cursorShape: "block" | "underline" | "beam" | "default";
	displayOffset: number;
	historySize: number;
	/** Lines evicted from the history top so far (monotonic within a resize era).
	 *  `historyBase + (historySize - displayOffset + screenRow)` is an
	 *  eviction-stable absolute row index — the key space for the scroll row cache. */
	historyBase: number;
	hasSelection: boolean;
	keyboardFlags: number;
	/** Alternate screen active. `historyBase` restarts from 0 on every alt
	 *  enter/exit, so the absolute-row cache MUST be dropped when this flips —
	 *  otherwise a primary-screen row can alias onto an alt row at the same key. */
	altScreen: boolean;
	/** DECCKM (application cursor keys) active. When true, unmodified
	 *  arrows/Home/End must be sent as SS3 (`\x1bO{A,B,C,D,H,F}`) instead of
	 *  CSI (`\x1b[{A,B,C,D,H,F}`). */
	appCursor: boolean;
	/** The app requested a steady (non-blinking) cursor via DECSCUSR (an even
	 *  Ps, e.g. `\x1b[2 q`). False also covers "no DECSCUSR seen" — today's
	 *  default is to blink a focused cursor. */
	cursorSteady: boolean;
	bell: boolean;
	mouseMode: 0 | 1 | 2 | 3;
	sgrMouse: boolean;
	focusReporting: boolean;
	bracketedPaste: boolean;
	screenRows: number;
	screenCols: number;
	rows: DecodedRow[];
}

/** Previous frame geometry/scroll state needed to decide what a new frame implies. */
export interface FrameGridPrev {
	lastScreenRows: number;
	lastScreenCols: number;
	lastDisplayOffset: number;
	lastHistorySize: number;
	lastAltScreen: boolean;
}

/** What a newly-decoded frame means for the rowMap. */
export interface FrameGridDecision {
	geomChanged: boolean;
	scrollChanged: boolean;
	/** Primary/alternate grid swap: all absolute row state belongs to a new era. */
	screenChanged: boolean;
	/** The frame carries a full screen of rows → replace the rowMap wholesale. */
	fullReplace: boolean;
	/** Partial frame after a scroll → clear and wait for a full frame; do NOT merge. */
	scrollWait: boolean;
}

/**
 * Decide what a decoded frame implies for the rowMap (geom/scroll/full-replace/
 * scroll-wait). Pure, so onFrame's grid bookkeeping is unit-testable away from the
 * CanvasTerminal closure.
 *
 * `fallbackRows` is the screen-row count to assume when the frame omits its own
 * (frame.screenRows === 0): onFrame passes lastResizeRows. The backend always sets
 * frame.screenRows in practice, so the fallback only differs on degenerate frames.
 */
/**
 * Map a DOM `MouseEvent.buttons` bitmask to an SGR mouse-report button code
 * for a *motion* report (xterm `?1002h`/`?1003h`): 0=left, 1=middle, 2=right,
 * 3="no button" — the code reserved for a bare hover. An app tracking its own
 * click-drag from motion reports (e.g. Claude Code's own text selection)
 * needs the actually-held button here to tell a drag from a hover; reporting
 * 3 unconditionally makes every drag look like a hover with nothing pressed.
 */
export function sgrMotionButton(buttons: number): 0 | 1 | 2 | 3 {
	if (buttons & 1) return 0;
	if (buttons & 4) return 1;
	if (buttons & 2) return 2;
	return 3;
}

export function decideFrameGrid(prev: FrameGridPrev, frame: DecodedFrame, fallbackRows: number): FrameGridDecision {
	const geomChanged = frame.screenRows !== prev.lastScreenRows || frame.screenCols !== prev.lastScreenCols;
	const scrollChanged = frame.displayOffset !== prev.lastDisplayOffset || frame.historySize !== prev.lastHistorySize;
	const screenChanged = frame.altScreen !== prev.lastAltScreen;
	const screenRowCount = frame.screenRows || fallbackRows || 24;
	const fullReplace = frame.rows.length >= screenRowCount;
	const scrollWait = !fullReplace && (screenChanged || (scrollChanged && !geomChanged));
	return { geomChanged, scrollChanged, screenChanged, fullReplace, scrollWait };
}

/** Inputs to the reconcile-fire gate (see shouldFireReconcile). */
export interface ReconcileGate {
	alive: boolean;
	isScrolling: boolean;
	/** Smooth-scroll fractional position; null when at rest on a line. */
	scrollPosF: number | null;
	/** Backend display offset of the current frame (0 = following output, <0 = no frame). */
	displayOffset: number;
}

/**
 * Whether a debounced full-frame reconciliation should actually fire.
 *
 * Partial frames merge into the rowMap by index, so a grid content shift can
 * strand stale rows (duplicate/vanished blocks) on the canvas while the grid
 * itself stays correct. scheduleReconcile() requests a full frame to self-heal,
 * but ONLY when the terminal is at rest and following output (offset 0). Firing
 * mid-gesture or while scrolled back would fight the active render or yank the
 * view. Pure, so the gate is unit-testable away from the CanvasTerminal closure.
 */
export function shouldFireReconcile(g: ReconcileGate): boolean {
	return g.alive && !g.isScrolling && g.scrollPosF == null && g.displayOffset === 0;
}

/**
 * Terminal grid dimensions for a pixel box — THE single source of truth shared
 * by CanvasTerminal's remeasure and Terminal's reconnect path. The width loses
 * the left gutter and the scrollbar strip before dividing into columns.
 *
 * Keeping both callers on this one formula matters: when the reconnect resize
 * (Terminal.tsx initSession) computed columns from the RAW width, it disagreed
 * with CanvasTerminal by ~2 cols, so every tab re-entry fired TWO SIGWINCHes
 * with different widths — and each one makes Ink (Claude Code) clear+reprint
 * its full frame, duplicating blocks into scrollback (mdkb
 * `ink-banner-dup-raw-ring-2026-07-06`). Identical dims instead hit the
 * backend's resize no-op guard: zero spurious SIGWINCH.
 */
export function gridDimsForBox(
	widthPx: number,
	heightPx: number,
	cellWidth: number,
	cellHeight: number,
): { rows: number; cols: number } {
	return {
		cols: Math.floor((widthPx - GUTTER_PX - SCROLLBAR_PX) / cellWidth),
		rows: Math.floor(heightPx / cellHeight),
	};
}

/** Trailing-debounce window for the full-frame reconcile self-heal. */
export const RECONCILE_DEBOUNCE_MS = 250;
/** Hard cap on how long a reschedule burst can defer the reconcile. */
export const RECONCILE_MAX_WAIT_MS = 1_000;

/**
 * Delay for the (re)scheduled reconcile timer: a plain trailing debounce
 * (RECONCILE_DEBOUNCE_MS) capped so the timer fires no later than
 * `burstStartedAt + RECONCILE_MAX_WAIT_MS`.
 *
 * Without the cap, sustained partial-frame output (an active agent repainting
 * every 16-33ms for minutes) resets the debounce on every frame and the
 * self-heal never fires — stale rows stranded on the canvas persist for the
 * whole burst (mdkb `ink-banner-dup-raw-ring-2026-07-06`). With the cap, the
 * heal runs at most ~1/s under continuous output and keeps the cheap trailing
 * behavior for short bursts. Pure, unit-tested.
 */
export function reconcileDelay(now: number, burstStartedAt: number): number {
	const deadline = burstStartedAt + RECONCILE_MAX_WAIT_MS;
	return Math.max(0, Math.min(RECONCILE_DEBOUNCE_MS, deadline - now));
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

	const numRows = view.getUint16(offset, true);
	offset += 2;
	const cursorRow = view.getUint16(offset, true);
	offset += 2;
	const cursorCol = view.getUint16(offset, true);
	offset += 2;
	const cursorVisible = view.getUint8(offset) !== 0;
	offset += 1;
	const displayOffset = view.getUint32(offset, true);
	offset += 4;
	const historySize = view.getUint32(offset, true);
	offset += 4;
	const hasSelection = view.getUint8(offset) !== 0;
	offset += 1;
	const rawKeyboardFlags = view.getUint8(offset);
	offset += 1;
	const frameFlags = view.getUint8(offset);
	offset += 1;
	const screenRows = view.getUint16(offset, true);
	offset += 2;
	const screenCols = view.getUint16(offset, true);
	offset += 2;
	const historyBase = view.getUint32(offset, true);
	offset += 4;
	// bits 5-7 of keyboard_flags ride along as alt-screen / app-cursor /
	// cursor-steady state, not keyboard flags — they land here because
	// frame_flags is full (see serialize_dirty_rows).
	const altScreen = (rawKeyboardFlags & 0x20) !== 0;
	const appCursor = (rawKeyboardFlags & 0x40) !== 0;
	const cursorSteady = (rawKeyboardFlags & 0x80) !== 0;
	const keyboardFlags = rawKeyboardFlags & 0x1f;
	const bell = (frameFlags & 0x01) !== 0;
	const cursorShapeRaw = (frameFlags >> 1) & 0x03;
	const cursorShape: "block" | "underline" | "beam" | "default" =
		cursorShapeRaw === 3
			? "default"
			: cursorShapeRaw === 2
				? "beam"
				: cursorShapeRaw === 1
					? "underline"
					: "block";
	const mouseMode = ((frameFlags >> 3) & 0x03) as 0 | 1 | 2 | 3;
	const sgrMouse = (frameFlags & 0x20) !== 0;
	const focusReporting = (frameFlags & 0x40) !== 0;
	const bracketedPaste = (frameFlags & 0x80) !== 0;

	const rows: DecodedRow[] = [];
	for (let r = 0; r < numRows; r++) {
		if (offset + 4 > buffer.byteLength) break;
		const rowIndex = view.getUint16(offset, true);
		offset += 2;
		const rawColCount = view.getUint16(offset, true);
		offset += 2;
		const wrapped = (rawColCount & ROW_WRAPPED_FLAG) !== 0;
		const colCount = rawColCount & ~ROW_WRAPPED_FLAG;

		const codepoints = new Uint32Array(colCount);
		const fg = new Uint32Array(colCount);
		const bg = new Uint32Array(colCount);
		const attrs = new Uint8Array(colCount);

		for (let c = 0; c < colCount; c++) {
			if (offset + CELL_SIZE > buffer.byteLength) break;
			codepoints[c] = view.getUint32(offset, true);
			offset += 4;
			const fgR = view.getUint8(offset++);
			const fgG = view.getUint8(offset++);
			const fgB = view.getUint8(offset++);
			const bgR = view.getUint8(offset++);
			const bgG = view.getUint8(offset++);
			const bgB = view.getUint8(offset++);
			attrs[c] = view.getUint8(offset++);
			fg[c] = (fgR << 16) | (fgG << 8) | fgB;
			bg[c] = (bgR << 16) | (bgG << 8) | bgB;
		}

		rows.push({ index: rowIndex, count: colCount, wrapped, codepoints, fg, bg, attrs });
	}

	return {
		cursorRow,
		cursorCol,
		cursorVisible,
		cursorShape,
		displayOffset,
		historySize,
		historyBase,
		hasSelection,
		keyboardFlags,
		altScreen,
		appCursor,
		cursorSteady,
		bell,
		mouseMode,
		sgrMouse,
		focusReporting,
		bracketedPaste,
		screenRows,
		screenCols,
		rows,
	};
}

/** A styled row tagged with its absolute index (0 = oldest scrollback line). */
export interface StyledRangeRow {
	abs: number;
	row: DecodedRow;
}

/** A decoded range of styled rows, feeding the client-side scroll row cache. */
export interface StyledRange {
	startAbs: number;
	historySize: number;
	cols: number;
	rows: StyledRangeRow[];
}

/**
 * Decode the styled-row-range payload from `terminal_styled_rows`.
 * Layout mirrors the Rust `serialize_styled_range`: start_abs u32,
 * history_size u32, cols u16, row_count u16, then per row abs u32 + colCount u16
 * + cells (colCount × CELL_SIZE).
 */
export function decodeStyledRange(buffer: ArrayBuffer): StyledRange | null {
	if (buffer.byteLength < 12) return null;
	const view = new DataView(buffer);
	let offset = 0;
	const startAbs = view.getUint32(offset, true);
	offset += 4;
	const historySize = view.getUint32(offset, true);
	offset += 4;
	const cols = view.getUint16(offset, true);
	offset += 2;
	const rowCount = view.getUint16(offset, true);
	offset += 2;

	const rows: StyledRangeRow[] = [];
	for (let r = 0; r < rowCount; r++) {
		if (offset + 6 > buffer.byteLength) break;
		const abs = view.getUint32(offset, true);
		offset += 4;
		const rawColCount = view.getUint16(offset, true);
		offset += 2;
		const wrapped = (rawColCount & ROW_WRAPPED_FLAG) !== 0;
		const colCount = rawColCount & ~ROW_WRAPPED_FLAG;
		const codepoints = new Uint32Array(colCount);
		const fg = new Uint32Array(colCount);
		const bg = new Uint32Array(colCount);
		const attrs = new Uint8Array(colCount);
		for (let c = 0; c < colCount; c++) {
			if (offset + CELL_SIZE > buffer.byteLength) break;
			codepoints[c] = view.getUint32(offset, true);
			offset += 4;
			const fgR = view.getUint8(offset++);
			const fgG = view.getUint8(offset++);
			const fgB = view.getUint8(offset++);
			const bgR = view.getUint8(offset++);
			const bgG = view.getUint8(offset++);
			const bgB = view.getUint8(offset++);
			attrs[c] = view.getUint8(offset++);
			fg[c] = (fgR << 16) | (fgG << 8) | fgB;
			bg[c] = (bgR << 16) | (bgG << 8) | bgB;
		}
		rows.push({ abs, row: { index: 0, count: colCount, wrapped, codepoints, fg, bg, attrs } });
	}
	return { startAbs, historySize, cols, rows };
}

/** Snap lineHeight to integer device pixels to prevent sub-pixel seams between rows. */
export function snapLineHeight(fontSize: number, target: number = 1.2): number {
	const dpr = window.devicePixelRatio || 1;
	const rawDevicePx = fontSize * target * dpr;
	const lo = Math.floor(rawDevicePx);
	const hi = Math.ceil(rawDevicePx);
	const best = Math.abs(rawDevicePx - lo) <= Math.abs(rawDevicePx - hi) ? lo : hi;
	const snapped = best / (fontSize * dpr);
	return Math.max(1.0, Math.min(snapped, 1.5));
}

export type CursorShape = "block" | "beam" | "underline";

/**
 * Resolve the shape to paint from the frame's DECSCUSR state and the user's
 * cursor-style setting. An app-requested shape (including an explicit "block")
 * always wins — matching iTerm2/Alacritty/kitty/WezTerm — and is what makes a
 * vi normal-mode block cursor visible. "default" means the app never sent
 * DECSCUSR, so the user's setting applies.
 */
export function resolveCursorShape(frameShape: DecodedFrame["cursorShape"], settingShape: CursorShape): CursorShape {
	return frameShape === "default" ? settingShape : frameShape;
}

/**
 * Whether the cursor should be painted on this tick, given the blink phase
 * and whether the app requested a steady (non-blinking) cursor via DECSCUSR.
 * A steady request always paints — it opts out of the blink cycle entirely,
 * rather than just starting from a different phase.
 */
export function shouldPaintCursor(cursorBlinkOn: boolean, cursorSteady: boolean): boolean {
	return cursorSteady || cursorBlinkOn;
}

export interface CursorRect {
	x: number;
	y: number;
	w: number;
	h: number;
}

/**
 * Compute the pixel rectangle for a cursor at the given grid position.
 *
 * `spanCols` widens a block/underline cursor to cover a wide glyph (CJK,
 * emoji, …) that bleeds into the next cell — the wire protocol carries no
 * "is this cell wide" flag (the spacer cell it occupies is indistinguishable
 * from a genuinely empty cell), so the caller determines width by measuring
 * the actual glyph (see `isWideCursorGlyph`) rather than from frame data. A
 * beam cursor stays a fixed-width insertion marker regardless, matching how
 * other terminals render it.
 */
export function computeCursorRect(
	shape: CursorShape,
	row: number,
	col: number,
	m: CellMetrics,
	spanCols: 1 | 2 = 1,
): CursorRect {
	const x = col * m.cellWidth;
	const y = row * m.cellHeight;
	switch (shape) {
		case "block":
			return { x, y, w: m.cellWidth * spanCols, h: m.cellHeight };
		case "beam":
			return { x, y, w: 2, h: m.cellHeight };
		case "underline":
			return { x, y: y + m.cellHeight - 2, w: m.cellWidth * spanCols, h: 2 };
	}
}

/**
 * Whether a glyph measured at `measuredWidthPx` occupies two grid columns
 * rather than one, given the terminal's cell width. Driven by an actual
 * canvas `measureText` result (not a Unicode East-Asian-Width table) because
 * that is what the renderer already does to lay the glyph out — it stays
 * correct for exactly the same set of characters the renderer treats as wide,
 * with no separate width table to keep in sync.
 */
export function isWideCursorGlyph(measuredWidthPx: number, cellWidth: number): boolean {
	return measuredWidthPx > cellWidth * 1.5;
}

/**
 * Measure a monospace font and return cell metrics for grid layout.
 * Matches xterm.js WebGL renderer dimension calculation exactly:
 *   device.char.height = ceil(charHeight * dpr)
 *   device.cell.height = floor(device.char.height * lineHeight)
 *   charTop = round((cellHeight_device - charHeight_device) / 2)
 */
export function measureFont(
	ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
	fontSize: number,
	fontFamily: string,
	dpr: number = 1,
	lineHeight: number = 1.2,
	fontWeight: number = 400,
	charHeightOverride?: number,
): CellMetrics {
	ctx.font = `${fontWeight} ${fontSize}px ${fontFamily}`;
	const m = ctx.measureText("W");
	const cellWidth = Math.round(m.width);

	const ascent = m.fontBoundingBoxAscent ?? m.actualBoundingBoxAscent;
	const descent = m.fontBoundingBoxDescent ?? m.actualBoundingBoxDescent;
	const charHeightCSS = charHeightOverride ?? ascent + descent;

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
