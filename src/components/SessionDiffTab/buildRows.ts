import type { EditStep, FileReview, SessionReview } from "../../types/sessionDiff";

export type SessionReviewMode = "file" | "chronological";

export type SessionRow =
	| { kind: "file"; group: FileReview; steps: EditStep[]; expanded: boolean; stepsOpen: boolean }
	| { kind: "step"; step: EditStep };

/**
 * PURE row-model builder for `SessionDiffList` — kept separate from the
 * virtualized rendering so the interesting logic (grouped vs chronological,
 * resolving a file's steps) is unit-testable without mounting anything.
 *
 * `expandedFiles`/`openStepFiles` are keyed by `abs_path` (a stable identity
 * across a revert-triggered refetch), never by array position.
 */
export function buildRows(
	review: SessionReview,
	mode: SessionReviewMode,
	expandedFiles: ReadonlySet<string>,
	openStepFiles: ReadonlySet<string>,
): SessionRow[] {
	if (mode === "chronological") {
		return [...review.steps].sort((a, b) => a.step_index - b.step_index).map((step) => ({ kind: "step", step }));
	}

	// Resolve each file's step_indices against the CURRENT steps array by
	// step_index identity, not array position — a dangling index (shouldn't
	// happen, but the backend's own contract doesn't rule it out) is dropped
	// rather than crashing the row build.
	const byIndex = new Map<number, EditStep>();
	for (const step of review.steps) byIndex.set(step.step_index, step);

	return review.files.map((group) => {
		const steps = group.step_indices.map((i) => byIndex.get(i)).filter((s): s is EditStep => s !== undefined);
		return {
			kind: "file",
			group,
			steps,
			expanded: expandedFiles.has(group.abs_path),
			stepsOpen: openStepFiles.has(group.abs_path),
		};
	});
}
