/**
 * Pure helpers that classify what changed between two fetches of the same
 * review (Session Diff's `SessionReview`, or Branch Diff Scroll's per-file
 * `DiffFileSection[]`) — kept separate from any component so the live-update
 * UI's "what should flash, what should hold behind Refresh" logic is
 * unit-testable without mounting a virtualized list.
 */

/** Anything identifiable by a stable key with a comparable revision/content
 *  fingerprint — `FileReview` (key = `abs_path`, fingerprint = `revision`) and
 *  Branch Diff's `DiffFileSection` (key = its `fileRowKeys()` row key,
 *  fingerprint = its own raw diff text, which changes iff the content does)
 *  both already fit this shape without adapting either type. */
export interface FingerprintedItem {
	key: string;
	fingerprint: string;
}

export interface FileDiffClassification {
	/** Keys present in `next` but not `prev` — always safe to apply immediately
	 *  (a brand-new row extends the list rather than mutating one in place). */
	newKeys: string[];
	/** Keys present in both, fingerprint changed, AND currently on screen —
	 *  apply immediately with a flash. */
	changedVisible: string[];
	/** Keys present in both, fingerprint changed, NOT currently on screen —
	 *  hold behind a "Refresh (N)" affordance rather than silently mutating
	 *  something the user isn't looking at. */
	changedHidden: string[];
}

/**
 * Classifies `next` against `prev` by key+fingerprint. A key missing from
 * `next` (a file reverted away, or otherwise dropped) is not reported here —
 * callers that need to detect removal do so separately by set difference,
 * since neither current caller (Session Diff, Branch Diff Scroll) needs to
 * distinguish "removed" from "just not new/changed" for this feature.
 */
export function classifyFingerprintedDiff(
	prev: ReadonlyArray<FingerprintedItem>,
	next: ReadonlyArray<FingerprintedItem>,
	visibleKeys: ReadonlySet<string>,
): FileDiffClassification {
	const prevByKey = new Map(prev.map((item) => [item.key, item.fingerprint]));
	const newKeys: string[] = [];
	const changedVisible: string[] = [];
	const changedHidden: string[] = [];

	for (const item of next) {
		const prevFingerprint = prevByKey.get(item.key);
		if (prevFingerprint === undefined) {
			newKeys.push(item.key);
		} else if (prevFingerprint !== item.fingerprint) {
			(visibleKeys.has(item.key) ? changedVisible : changedHidden).push(item.key);
		}
	}

	return { newKeys, changedVisible, changedHidden };
}

/** Chronological mode's analog: a step has no revision/fingerprint of its own
 *  (it's immutable once transcribed — a step never "changes", it only either
 *  exists yet or doesn't), so this is just set difference by `tool_use_id`,
 *  in the order they appear in `next`. */
export function classifyNewSteps<T extends { tool_use_id: string }>(prev: readonly T[], next: readonly T[]): T[] {
	const prevIds = new Set(prev.map((s) => s.tool_use_id));
	return next.filter((s) => !prevIds.has(s.tool_use_id));
}
