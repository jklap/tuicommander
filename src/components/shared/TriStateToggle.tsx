import { type Component, For } from "solid-js";
import { t } from "../../i18n";
import s from "./TriStateToggle.module.css";

export interface TriStateToggleProps {
	/** null = inherit the global default */
	value: boolean | null;
	onChange: (value: boolean | null) => void;
	label: string;
	/** Resolved global value, shown in the "Use global default (…)" hint while value is null */
	inherited: boolean;
	/** Override the "On" segment text (default "On") */
	onLabel?: string;
	/** Override the "Off" segment text (default "Off") */
	offLabel?: string;
}

/** Left-to-right segment order: Off, Use global, On */
const ORDER: (boolean | null)[] = [false, null, true];

function segmentText(value: boolean | null, props: TriStateToggleProps): string {
	if (value === null) return t("triStateToggle.global", "Global");
	return value ? (props.onLabel ?? t("triStateToggle.on", "On")) : (props.offLabel ?? t("triStateToggle.off", "Off"));
}

/**
 * Three-position On / Use global / Off control for a boolean setting that can
 * inherit from a global default. Unlike a plain checkbox — which can only ever
 * write a concrete true/false and therefore has no way back to "inherit" once
 * touched — every state here, including "use global", is directly selectable.
 *
 * A `role="radiogroup"` of three `role="radio"` segments with a roving
 * tabindex: Left/Right (or Up/Down) move AND select, matching native radio
 * button behavior, so screen readers announce both the group's label and
 * which of the three positions is checked.
 */
export const TriStateToggle: Component<TriStateToggleProps> = (props) => {
	// Roving tabindex per WAI-ARIA APG: an arrow key must move both the
	// selection AND the DOM focus together, or the :focus-visible outline
	// and the checked segment visibly desync (outline stays on the segment
	// you started on; the highlight jumps to a different one).
	const buttonRefs: (HTMLButtonElement | undefined)[] = [];

	const handleKeyDown = (e: KeyboardEvent) => {
		const idx = ORDER.indexOf(props.value);
		let next = idx;
		if (e.key === "ArrowRight" || e.key === "ArrowDown") {
			e.preventDefault();
			next = Math.min(idx + 1, ORDER.length - 1);
		} else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
			e.preventDefault();
			next = Math.max(idx - 1, 0);
		} else {
			return;
		}
		if (next !== idx) {
			props.onChange(ORDER[next]);
			buttonRefs[next]?.focus();
		}
	};

	return (
		<div class={s.triToggle}>
			<div class={s.track} role="radiogroup" aria-label={props.label} onKeyDown={handleKeyDown}>
				<For each={ORDER}>
					{(segValue, index) => {
						const selected = () => props.value === segValue;
						const kind = segValue === null ? "global" : segValue ? "on" : "off";
						return (
							<button
								ref={(el) => {
									buttonRefs[index()] = el;
								}}
								type="button"
								role="radio"
								aria-checked={selected()}
								tabIndex={selected() ? 0 : -1}
								class={s.segment}
								data-kind={kind}
								data-selected={selected() ? "" : undefined}
								onClick={() => props.onChange(segValue)}
							>
								{segmentText(segValue, props)}
							</button>
						);
					}}
				</For>
			</div>
			<span class={s.label}>
				{props.label}
				{props.value === null && (
					<span class={s.stateHint}>
						{" "}
						{t("triStateToggle.useGlobalDefault", "(Use global default: {value})", {
							value: segmentText(props.inherited, props),
						})}
					</span>
				)}
			</span>
		</div>
	);
};
