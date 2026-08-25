/**
 * Shared binary-frame builder for tests that need a real `decodeBinaryFrame()`
 * payload rather than hand-rolling the wire format inline. Mirrors the format
 * documented in `src-tauri/src/terminal_grid.rs` (`serialize_dirty_rows`) and
 * matches the byte-for-byte layout already hand-built in
 * `frameWrappedRows.test.ts` / `frameAppCursorAndShape.test.ts` — those two
 * files build bytes by hand ON PURPOSE (their docblocks say so: they're the
 * only tests that fail if one half of the wire format moves without the
 * other) and are NOT migrated onto this helper. Everything else that just
 * needs *a* valid frame — the mount harness, future decoder tests — should
 * use this instead of re-deriving the offsets a fourth time.
 */

export const FRAME_HEADER_SIZE = 26;
export const FRAME_CELL_SIZE = 11; // 4 (char u32) + 3 (fg) + 3 (bg) + 1 (attrs)
export const ROW_WRAPPED_FLAG = 0x8000;

export interface FrameRowSpec {
	text: string;
	/** Pads/truncates the row to this many columns; defaults to the frame's `cols`. */
	cols?: number;
	wrapped?: boolean;
}

export interface FrameSpec {
	rows: FrameRowSpec[];
	/** Column count assumed for rows that don't specify their own. Also becomes
	 *  the frame's `screenCols` unless overridden. */
	cols: number;
	cursorRow?: number;
	cursorCol?: number;
	cursorVisible?: boolean;
	displayOffset?: number;
	historySize?: number;
	hasSelection?: boolean;
	keyboardFlags?: number;
	frameFlags?: number;
	/** Defaults to `rows.length` — override to simulate a partial frame. */
	screenRows?: number;
	screenCols?: number;
	historyBase?: number;
}

function writeRowCells(view: DataView, offset: number, text: string, cols: number): number {
	const padded = text.length >= cols ? text.slice(0, cols) : text.padEnd(cols, " ");
	for (const char of padded) {
		view.setUint32(offset, char.codePointAt(0) ?? 0, true);
		offset += 4;
		offset += 7; // fg rgb, bg rgb, attrs — zeroed, irrelevant to text-only fixtures
	}
	return offset;
}

/** Build one binary frame buffer per the wire format `decodeBinaryFrame` expects. */
export function buildFrame(spec: FrameSpec): ArrayBuffer {
	const cols = spec.cols;
	const rowCount = spec.rows.length;
	const totalRowBytes = spec.rows.reduce((sum, row) => sum + 4 + (row.cols ?? cols) * FRAME_CELL_SIZE, 0);
	const buffer = new ArrayBuffer(FRAME_HEADER_SIZE + totalRowBytes);
	const view = new DataView(buffer);

	view.setUint16(0, rowCount, true); // num_rows (rows carried in THIS frame)
	view.setUint16(2, spec.cursorRow ?? 0, true);
	view.setUint16(4, spec.cursorCol ?? 0, true);
	view.setUint8(6, spec.cursorVisible === false ? 0 : 1);
	view.setUint32(7, spec.displayOffset ?? 0, true);
	view.setUint32(11, spec.historySize ?? 0, true);
	view.setUint8(15, spec.hasSelection ? 1 : 0);
	view.setUint8(16, spec.keyboardFlags ?? 0);
	view.setUint8(17, spec.frameFlags ?? 0);
	view.setUint16(18, spec.screenRows ?? rowCount, true); // num_lines
	view.setUint16(20, spec.screenCols ?? cols, true); // num_cols
	view.setUint32(22, spec.historyBase ?? 0, true);

	let offset = FRAME_HEADER_SIZE;
	spec.rows.forEach((row, index) => {
		const rowCols = row.cols ?? cols;
		view.setUint16(offset, index, true);
		offset += 2;
		view.setUint16(offset, rowCols | (row.wrapped ? ROW_WRAPPED_FLAG : 0), true);
		offset += 2;
		offset = writeRowCells(view, offset, row.text, rowCols);
	});

	return buffer;
}

/** Convenience: a single-screen full-replace frame from plain text lines. */
export function buildTextFrame(lines: string[], cols = 80, overrides: Partial<FrameSpec> = {}): ArrayBuffer {
	return buildFrame({
		rows: lines.map((text) => ({ text })),
		cols,
		...overrides,
	});
}
