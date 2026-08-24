import type { CommandBlock } from "../../stores/terminals";
import type { SearchMatch } from "./canvasTerminalSelection";

/** Minimal shape of a command block this module needs — avoids importing the whole store's
 *  `CommandBlock` surface into callers that only have a plain object (e.g. tests). */
export type MarkBlock = Pick<CommandBlock, "promptLine" | "endLine" | "exitCode">;

export interface ScrollbarMarksInput {
	showBlockMarks: boolean;
	showPromptMarks: boolean;
	blocks: readonly MarkBlock[];
	promptLines: readonly number[];
	totalRows: number;
	matches: readonly SearchMatch[];
}

/**
 * Memo key for `scrollbarMarksHtml`. Each toggle's contribution collapses to a fixed
 * placeholder when that category is hidden, so the key doesn't churn on invisible changes —
 * but the toggle flip itself always changes the count term (real count vs. 0), so re-enabling
 * always invalidates the memo even if blocks/prompts/totalRows are otherwise unchanged since
 * it was hidden. (Regression: the previous key collapsed both counts to 0 while EITHER
 * category was hidden, sharing one `showBlocks` flag, so it could go stale across a
 * hide/show cycle of either toggle independently.)
 */
export function scrollbarMarksKey(input: ScrollbarMarksInput): string {
	const { showBlockMarks, showPromptMarks, blocks, promptLines, totalRows, matches } = input;
	const lastBlock = blocks[blocks.length - 1];
	const lastPrompt = promptLines[promptLines.length - 1];
	const searchCount = matches.length;
	return (
		`b${showBlockMarks ? blocks.length : 0}:${showBlockMarks ? (lastBlock?.promptLine ?? "") : ""}:${showBlockMarks ? (lastBlock?.endLine ?? "") : ""}:${showBlockMarks ? (lastBlock?.exitCode ?? "") : ""}` +
		`:p${showPromptMarks ? promptLines.length : 0}:${showPromptMarks ? (lastPrompt ?? "") : ""}` +
		`:t${totalRows}` +
		`:s${searchCount}:${searchCount > 0 ? matches[0].row : ""}`
	);
}

/**
 * Scrollbar tick markup for command blocks (blue/red), user-prompt lines (green), and search
 * matches (orange). Block ticks are drawn first, prompt ticks after (so they sit on top).
 */
export function scrollbarMarksHtml(input: ScrollbarMarksInput, trackH: number): string {
	const { showBlockMarks, showPromptMarks, blocks, promptLines, totalRows, matches } = input;
	let html = "";
	if (showBlockMarks) {
		for (const block of blocks) {
			const ratio = block.promptLine / totalRows;
			const color = block.exitCode !== null && block.exitCode !== 0 ? "#f85149" : "rgba(88,166,255,0.5)";
			html += `<div style="position:absolute;right:0;width:100%;height:2px;top:${ratio * trackH}px;background:${color}"></div>`;
		}
	}
	if (showPromptMarks) {
		// Dedicated GREEN tick at each line where the USER submitted a prompt
		// (distinct from the blue/red agent tool-call block ticks above): few,
		// one per turn. Drawn after the block ticks so it sits on top.
		for (const line of promptLines) {
			const ratio = line / totalRows;
			html += `<div style="position:absolute;right:0;width:100%;height:2px;top:${ratio * trackH}px;background:#3fb950"></div>`;
		}
	}
	if (matches.length > 0) {
		const seen = new Set<number>();
		for (const match of matches) {
			const rounded = Math.round((match.row / totalRows) * trackH);
			if (seen.has(rounded)) continue;
			seen.add(rounded);
			html += `<div style="position:absolute;right:0;width:100%;height:2px;top:${rounded}px;background:#e8984c"></div>`;
		}
	}
	return html;
}
