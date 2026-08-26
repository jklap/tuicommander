/**
 * Pick the command block to jump to from the current viewport position.
 *
 * `promptLines` should include every known block's `promptLine` (both
 * `commandBlocks` and a still-open `activeBlock`, if any) — the caller
 * concatenates them, this function doesn't care which block a line belongs to.
 *
 * "previous" returns the closest prompt line strictly above `viewTop`;
 * "next" returns the closest strictly below. Returns `undefined` when there
 * is no such block (e.g. already at the first/last one).
 */
export function pickBlock(promptLines: number[], viewTop: number, direction: "previous" | "next"): number | undefined {
	if (direction === "previous") {
		for (let i = promptLines.length - 1; i >= 0; i--) {
			if (promptLines[i] < viewTop) return promptLines[i];
		}
		return undefined;
	}
	for (let i = 0; i < promptLines.length; i++) {
		if (promptLines[i] > viewTop) return promptLines[i];
	}
	return undefined;
}
