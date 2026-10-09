import type { EditStep, FileReview, SessionReview } from "../../types/sessionDiff";

export type SessionReviewMode = "file" | "chronological";

/** One step, plus the per-row state `SessionDiffTab` owns (collapse) and the
 *  jump targets `buildRows` bakes in once per build rather than making every
 *  consumer re-derive them from `review` on every render. */
export interface StepRowData {
	step: EditStep;
	collapsed: boolean;
	/** The step_index of the previous/next step touching the SAME file,
	 *  chronologically — `null` when this is the first/last touch. Only
	 *  meaningful in chronological mode (`StepCard`'s `showFilePath`); still
	 *  computed for by-file mode's nested rows, just unused there. */
	prevSameFileStepIndex: number | null;
	nextSameFileStepIndex: number | null;
}

export type SessionRow =
	| { kind: "file"; group: FileReview; steps: StepRowData[]; expanded: boolean; stepsOpen: boolean }
	| ({ kind: "step" } & StepRowData);

/**
 * Stable identity for a row, independent of its position in the array.
 * Used both by the virtualizer's `getItemKey` (which slot gets reused) and by
 * `SessionDiffList`'s per-row `<Show keyed>` (when to remount vs. update in
 * place) — the two MUST agree, or a slot can be "reused" by the virtualizer
 * while the row content thinks it's a different item, or vice versa.
 */
export function rowKey(row: SessionRow): string {
	return row.kind === "file" ? `f:${row.group.abs_path}` : `s:${row.step.tool_use_id}`;
}

/**
 * Given a step, finds the step_index of the previous/next step touching the
 * SAME file, in chronological order — powers `StepCard`'s `^`/`v` "jump to
 * the other time this file changed" buttons. Pure and unit-testable in
 * isolation from any rendering: looks the file up in `review.files` (whose
 * `step_indices` are already chronological) and walks to the step's own
 * neighbors there, rather than re-scanning `review.steps` linearly.
 */
export function findAdjacentStepIndices(
	review: SessionReview,
	step: EditStep,
): { prevStepIndex: number | null; nextStepIndex: number | null } {
	const fileReview = review.files.find((f) => f.abs_path === step.abs_path);
	if (!fileReview) return { prevStepIndex: null, nextStepIndex: null };
	const indices = fileReview.step_indices;
	const pos = indices.indexOf(step.step_index);
	if (pos === -1) return { prevStepIndex: null, nextStepIndex: null };
	return {
		prevStepIndex: pos > 0 ? indices[pos - 1] : null,
		nextStepIndex: pos < indices.length - 1 ? indices[pos + 1] : null,
	};
}

/**
 * Resolves a `step_index` (the backend's stable-within-a-snapshot identity,
 * per `EditStep.step_index`'s own doc comment) to a row position in the
 * CURRENT `rows` array — used by the turn picker and the `^`/`v` same-file
 * jump buttons to drive `DiffListNavHandle.scrollToIndex`. Only chronological
 * mode's rows are steps 1:1, so this returns `null` in by-file mode (there is
 * no single "the row for this step" when steps are nested under a file row).
 */
export function rowIndexForStepIndex(rows: SessionRow[], stepIndex: number): number | null {
	for (let i = 0; i < rows.length; i++) {
		const row = rows[i];
		if (row.kind === "step" && row.step.step_index === stepIndex) return i;
	}
	return null;
}

function toStepRowData(step: EditStep, review: SessionReview, collapsedSteps: ReadonlySet<string>): StepRowData {
	const { prevStepIndex, nextStepIndex } = findAdjacentStepIndices(review, step);
	return {
		step,
		collapsed: collapsedSteps.has(step.tool_use_id),
		prevSameFileStepIndex: prevStepIndex,
		nextSameFileStepIndex: nextStepIndex,
	};
}

/**
 * PURE row-model builder for `SessionDiffList` — kept separate from the
 * virtualized rendering so the interesting logic (grouped vs chronological,
 * resolving a file's steps) is unit-testable without mounting anything.
 *
 * `expandedFiles`/`openStepFiles`/`collapsedSteps` are keyed by `abs_path`/
 * `tool_use_id` (a stable identity across a revert-triggered refetch), never
 * by array position.
 */
export function buildRows(
	review: SessionReview,
	mode: SessionReviewMode,
	expandedFiles: ReadonlySet<string>,
	openStepFiles: ReadonlySet<string>,
	collapsedSteps: ReadonlySet<string>,
): SessionRow[] {
	if (mode === "chronological") {
		return [...review.steps]
			.sort((a, b) => a.step_index - b.step_index)
			.map((step) => ({ kind: "step", ...toStepRowData(step, review, collapsedSteps) }));
	}

	// Resolve each file's step_indices against the CURRENT steps array by
	// step_index identity, not array position — a dangling index (shouldn't
	// happen, but the backend's own contract doesn't rule it out) is dropped
	// rather than crashing the row build.
	const byIndex = new Map<number, EditStep>();
	for (const step of review.steps) byIndex.set(step.step_index, step);

	return review.files.map((group) => {
		const steps = group.step_indices
			.map((i) => byIndex.get(i))
			.filter((s): s is EditStep => s !== undefined)
			.map((step) => toStepRowData(step, review, collapsedSteps));
		return {
			kind: "file",
			group,
			steps,
			expanded: expandedFiles.has(group.abs_path),
			stepsOpen: openStepFiles.has(group.abs_path),
		};
	});
}
