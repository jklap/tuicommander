import type { DecodedRow, StyledRange } from "./canvasTerminalUtils";
import { cellText } from "./canvasTerminalUtils";
import { ANSWER_MARKER_RE, type RowSnapshot } from "./suggestOverlay";

/** The compact view of the last assistant turn: what the user asked, then only the 💬 answers. */
export interface AnswersTurn {
	prompt: string | null;
	answers: string[];
}

/** Rows fetched per `terminal_styled_rows` call. */
export const TURN_FETCH_CHUNK = 256;
/** Rows read back when no user prompt is known, so an unbounded scrollback is never swept. */
export const TURN_MAX_ROWS = 5000;

// Extended pictographs plus the East Asian wide blocks (Hangul Jamo, CJK, Hangul syllables,
// compatibility ideographs and forms, fullwidth forms, supplementary ideographs).
const WIDE_CHAR =
	/\p{Extended_Pictographic}|[\u1100-\u115F\u2E80-\uA4CF\uAC00-\uD7A3\uF900-\uFAFF\uFE30-\uFE6F\uFF00-\uFF60\uFFE0-\uFFE6]|[\u{20000}-\u{3FFFD}]/u;

/**
 * Text of a row for copying. The wire encodes the spacer cell after a wide
 * character as codepoint 0, which `rowText` renders as a space; here it is dropped
 * so "💬 x" does not become "💬  x".
 */
export function rowCopyText(row: DecodedRow): string {
	let text = "";
	for (let col = 0; col < row.count; col++) {
		const contents = cellText(row, col);
		if (contents === "" && col > 0 && row.codepoints[col - 1] !== 0 && WIDE_CHAR.test(cellText(row, col - 1))) continue;
		text += contents === "" ? " " : contents;
	}
	return text;
}

/** Join the soft-wrapped rows starting at `from` into one logical line; returns it and the next row. */
function joinLogicalLine(rows: readonly RowSnapshot[], from: number): { text: string; next: number } {
	let text = rows[from].text;
	let next = from + 1;
	while (next < rows.length && rows[next].isWrapped) {
		text += rows[next].text;
		next++;
	}
	return { text: text.trimEnd(), next };
}

/**
 * Build the answers-only view from the rows of one turn. With `hasPrompt` the
 * first row is the user's prompt line; every logical line after it that starts
 * with the 💬 marker is an answer, in order, wrapped rows joined.
 */
export function buildAnswersTurn(rows: readonly RowSnapshot[], hasPrompt: boolean): AnswersTurn {
	let i = 0;
	let prompt: string | null = null;
	if (hasPrompt && rows.length > 0) {
		const line = joinLogicalLine(rows, 0);
		prompt = line.text;
		i = line.next;
	}
	const answers: string[] = [];
	while (i < rows.length) {
		const line = joinLogicalLine(rows, i);
		if (!rows[i].isWrapped && ANSWER_MARKER_RE.test(line.text))
			answers.push(line.text.trimStart().replace(/^[●⏺]\s*/, ""));
		i = line.next;
	}
	return { prompt, answers };
}

/**
 * Read the rows `[startAbs, endAbs)` (all-time row indexes) through `fetchRange`,
 * which is the same `terminal_styled_rows` reader the scroll row cache uses.
 * Rows the backend no longer holds come back as blank so the result stays aligned;
 * a failed chunk aborts the read (null) rather than yielding a partial turn.
 */
export async function readTurnRows(
	fetchRange: (start: number, count: number) => Promise<StyledRange | null>,
	startAbs: number,
	endAbs: number,
): Promise<RowSnapshot[] | null> {
	const byAbs = new Map<number, RowSnapshot>();
	for (let start = startAbs; start < endAbs; start += TURN_FETCH_CHUNK) {
		const range = await fetchRange(start, Math.min(TURN_FETCH_CHUNK, endAbs - start));
		if (!range) return null;
		for (const { abs, row } of range.rows) byAbs.set(abs, { text: rowCopyText(row), isWrapped: row.wrapped });
	}
	const rows: RowSnapshot[] = [];
	for (let abs = startAbs; abs < endAbs; abs++) rows.push(byAbs.get(abs) ?? { text: "", isWrapped: false });
	return rows;
}

/**
 * Where the last turn starts, as an all-time row index: the last user prompt at
 * or after the oldest retained row, else the oldest row within `TURN_MAX_ROWS`
 * of the end. `promptLines` are grid-relative; `historyBase` shifts them to all-time.
 */
export function turnStart(
	promptLines: readonly number[],
	historyBase: number,
	endAbs: number,
): { startAbs: number; hasPrompt: boolean } {
	const total = endAbs - historyBase;
	let last = -1;
	for (const line of promptLines) if (line >= 0 && line < total) last = Math.max(last, line);
	if (last >= 0) return { startAbs: historyBase + last, hasPrompt: true };
	return { startAbs: Math.max(historyBase, endAbs - TURN_MAX_ROWS), hasPrompt: false };
}
