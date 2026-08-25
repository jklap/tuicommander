import type { Component } from "solid-js";
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

/** Cycle order: Global (inherit) -> On -> Off -> Global */
function next(value: boolean | null): boolean | null {
	if (value === null) return true;
	if (value === true) return false;
	return null;
}

function stateName(value: boolean | null, props: TriStateToggleProps): string {
	if (value === null) return t("triStateToggle.global", "Global");
	return value ? (props.onLabel ?? t("triStateToggle.on", "On")) : (props.offLabel ?? t("triStateToggle.off", "Off"));
}

/**
 * Single cycling switch for a boolean setting that can inherit from a global
 * default. Unlike a plain checkbox — which can only ever write a concrete
 * true/false and therefore has no way back to "inherit" once touched — this
 * control cycles through all three states: Global -> On -> Off -> Global.
 *
 * Visually mirrors the standard on/off `SettingToggle` pill switch (see
 * `SettingFields.tsx`), with the inherited ("Global") position rendered
 * dashed/dimmed partway along the track so it reads as neither firmly on nor
 * off. `aria-checked="mixed"` is the ARIA-correct encoding for that inherited
 * state (`role="switch"` does not support `mixed`, hence `role="checkbox"`).
 */
export const TriStateToggle: Component<TriStateToggleProps> = (props) => {
	const state = () => (props.value === null ? "global" : props.value ? "on" : "off");
	const ariaChecked = () => (props.value === null ? "mixed" : props.value ? "true" : "false");

	const handleActivate = () => props.onChange(next(props.value));

	// A real browser already synthesizes a `click` on Enter/Space for a native
	// `<button>`, making this handler's own `onClick` call redundant there — but
	// this project's test environment (happy-dom, via @solidjs/testing-library)
	// does not implement that UA activation behavior: dispatching a raw
	// `keydown` there fires no click at all (confirmed empirically). Keeping
	// this explicit handler is what makes `fireEvent.keyDown` actually exercise
	// keyboard activation in tests, not dead-weight duplication.
	const handleKeyDown = (e: KeyboardEvent) => {
		if (e.key === " " || e.key === "Enter") {
			e.preventDefault();
			handleActivate();
		}
	};

	return (
		<div class={s.triToggle}>
			<button
				type="button"
				role="checkbox"
				aria-checked={ariaChecked()}
				aria-label={props.label}
				title={`${props.label}: ${stateName(props.value, props)}`}
				class={s.switch}
				data-state={state()}
				onClick={handleActivate}
				onKeyDown={handleKeyDown}
			/>
			<span class={s.label}>
				{props.label}
				{props.value === null && (
					<span class={s.stateHint}>
						{" "}
						{t("triStateToggle.useGlobalDefault", "(Use global default: {value})", {
							value: stateName(props.inherited, props),
						})}
					</span>
				)}
			</span>
		</div>
	);
};
