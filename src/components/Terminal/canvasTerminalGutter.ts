import type { CommandBlock } from "../../stores/terminals";

/** Minimal shape these pure helpers need — avoids importing the whole store's
 *  `CommandBlock` surface into callers that only have a plain object (e.g. tests). */
export type GutterBlock = Pick<CommandBlock, "promptLine" | "executionLine" | "exitCode">;

export type GutterMarkKind = "success" | "failure" | null;

/**
 * Which status mark (if any) `paintGutterMarkers` should draw for a block.
 * `exitCode === null` means the block hasn't closed yet (or has no exit-code
 * source) — no mark. Pulled out of the inline `continue` it used to be so the
 * success/failure decision has its own test coverage (issue #7: the gutter
 * drew red on failure but never green on success).
 */
export function gutterMarkKind(block: Pick<GutterBlock, "exitCode">): GutterMarkKind {
	if (block.exitCode === null) return null;
	return block.exitCode === 0 ? "success" : "failure";
}

export type GutterZone = "fold" | "copy";

/**
 * Which gesture a gutter click on `absRow` should trigger for `block`: the
 * block's header row (`executionLine ?? promptLine` — where the fold chevron
 * paints) folds/unfolds it, and the rest of the block's gutter run selects its
 * output for copying. Issue #6: gutter click never had a fold gesture at all;
 * this is the hit-test split that gives it one without losing the existing
 * copy behavior everywhere else in the block.
 */
export function gutterZoneAt(absRow: number, block: Pick<GutterBlock, "promptLine" | "executionLine">): GutterZone {
	const headerRow = block.executionLine ?? block.promptLine;
	return absRow === headerRow ? "fold" : "copy";
}

/**
 * Whether a fold-toggle gesture (gutter chevron click, `Cmd+Shift+.`) should
 * actually flip fold state. Unfolding is always safe. Folding ON is only
 * allowed when the block has something to fold (`hasFoldableContent`, i.e.
 * `foldRange(block) !== null`) — code-review follow-up on issues #1/#5: a
 * still-running block (no `endLine` yet) has no foldable content, and toggling
 * it on unconditionally silently pre-folded it before it ever appeared in
 * `commandBlocks`, so it rendered pre-folded the instant it closed with no
 * further action from the user. The same ungated toggle also let
 * `paintFoldChevrons` draw a permanent folded-looking chevron on a block with
 * nothing actually folded.
 */
export function canToggleFold(alreadyFolded: boolean, hasFoldableContent: boolean): boolean {
	return alreadyFolded || hasFoldableContent;
}
