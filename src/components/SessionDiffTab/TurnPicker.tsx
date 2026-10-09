import { type Component, createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import type { EditStep, TurnSummary } from "../../types/sessionDiff";
import { cx } from "../../utils";
import { formatRelativeTime } from "../../utils/formatRelativeTime";
import s from "./SessionDiffTab.module.css";

export interface TurnPickerProps {
	turns: TurnSummary[];
	/** `review.steps` — used only to build each turn's hover-tooltip snippet
	 *  (the first line or two of its first touched step's own patch). */
	steps: EditStep[];
	/** Called with the step_index to jump to — the FIRST step of the chosen
	 *  turn, resolved to a row by the caller via `rowIndexForStepIndex`. */
	onSelect: (stepIndex: number) => void;
}

function fileListLabel(files: string[]): string {
	if (files.length <= 2) return files.join(", ");
	return `${files.slice(0, 2).join(", ")} +${files.length - 2} more`;
}

/** Plain-text tooltip content — a native `title` attribute, not a rich
 *  hover overlay. Simple, and trivially assertable in tests (an element's
 *  `title` is just a DOM property), at the cost of no rich formatting; this
 *  is a deliberate simplification, not an oversight. */
function tooltipFor(turn: TurnSummary, steps: EditStep[]): string {
	const firstStepIndex = turn.step_indices[0];
	const firstStep = firstStepIndex !== undefined ? steps.find((s) => s.step_index === firstStepIndex) : undefined;
	const snippet = firstStep?.patch
		.split("\n")
		.filter((line) => line.startsWith("+") || line.startsWith("-"))
		.slice(0, 3)
		.join("\n");
	return [turn.prompt_preview, snippet].filter(Boolean).join("\n\n") || "(no prompt text)";
}

/** Chronological-mode toolbar dropdown listing every turn (one user prompt's
 *  worth of edits) — lets the reviewer jump straight to a turn instead of
 *  scrolling through every individual step. */
export const TurnPicker: Component<TurnPickerProps> = (props) => {
	const [open, setOpen] = createSignal(false);
	let panelRef: HTMLDivElement | undefined;

	createEffect(() => {
		if (!open()) return;
		const handleClickOutside = (e: MouseEvent) => {
			if (panelRef && !panelRef.contains(e.target as Node)) setOpen(false);
		};
		const handleEscape = (e: KeyboardEvent) => {
			if (e.key === "Escape") setOpen(false);
		};
		let attached = false;
		const rafId = requestAnimationFrame(() => {
			document.addEventListener("click", handleClickOutside);
			attached = true;
		});
		document.addEventListener("keydown", handleEscape);
		onCleanup(() => {
			cancelAnimationFrame(rafId);
			if (attached) document.removeEventListener("click", handleClickOutside);
			document.removeEventListener("keydown", handleEscape);
		});
	});

	const rows = createMemo(() =>
		props.turns.map((turn) => ({
			turn,
			time: turn.started_at ? formatRelativeTime(Date.now() - Date.parse(turn.started_at)) : "",
			files: fileListLabel(turn.files),
			tooltip: tooltipFor(turn, props.steps),
		})),
	);

	return (
		<div class={s.turnPicker} ref={panelRef}>
			<button
				type="button"
				class={s.modeBtn}
				onClick={() => setOpen((v) => !v)}
				title="Jump to a turn"
				disabled={props.turns.length === 0}
			>
				Turns ({props.turns.length})
			</button>
			<Show when={open()}>
				<div class={s.turnPickerPanel} data-testid="turn-picker-panel">
					<For each={rows()}>
						{(row) => (
							<button
								type="button"
								class={s.turnPickerItem}
								title={row.tooltip}
								onClick={() => {
									props.onSelect(row.turn.step_indices[0]);
									setOpen(false);
								}}
							>
								<span class={s.turnPickerTime}>{row.time}</span>
								<span class={s.turnPickerSize}>
									<span class={s.statAdd}>+{row.turn.additions}</span>{" "}
									<span class={s.statDel}>-{row.turn.deletions}</span>
								</span>
								<span class={cx(s.turnPickerFiles)}>{row.files}</span>
							</button>
						)}
					</For>
				</div>
			</Show>
		</div>
	);
};
