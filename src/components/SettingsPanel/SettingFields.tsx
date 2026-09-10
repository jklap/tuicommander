import { type Component, For, type JSX, Show } from "solid-js";
import s from "./Settings.module.css";

/** Stable DOM id for a setting's wrapper `<div class={s.group}>`, derived from
 *  its label — used by SettingsPanel's search feature (`settingsSearchIndex.ts`)
 *  to scrollIntoView + highlight the matched control. Must stay in sync with
 *  that file, which computes the same id to build its search corpus.
 *
 *  Labels are unique within a single rendered tab in practice, EXCEPT rows
 *  inside a `<For>` over a dynamic list (per-agent, per-account, per-provider,
 *  …) — those repeat the same label per row, so this deliberately produces
 *  duplicate ids there. That's a latent, harmless HTML-validity nit: nothing
 *  calls `getElementById` against those rows (the search index only indexes
 *  static, non-looped settings), so no lookup ever resolves to the wrong one.
 */
export function settingSlugId(label: string): string {
	const slug = label
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-+|-+$/g, "");
	return `setting-${slug}`;
}

export const SettingToggle: Component<{
	checked: boolean;
	onChange: (checked: boolean) => void;
	label: string;
	hint?: string;
	hintStyle?: JSX.CSSProperties;
}> = (props) => (
	<div class={s.group} id={settingSlugId(props.label)}>
		<div class={s.toggle}>
			<input type="checkbox" checked={props.checked} onChange={(e) => props.onChange(e.currentTarget.checked)} />
			<span>{props.label}</span>
		</div>
		<Show when={props.hint}>
			<p class={s.hint} style={props.hintStyle}>
				{props.hint}
			</p>
		</Show>
	</div>
);

export const SettingSelect: Component<{
	label: string;
	value: string;
	onChange: (value: string) => void;
	options: { value: string; label: string }[];
	hint?: string;
	hintStyle?: JSX.CSSProperties;
}> = (props) => (
	<div class={s.group} id={settingSlugId(props.label)}>
		<label>{props.label}</label>
		<select value={props.value} onChange={(e) => props.onChange(e.currentTarget.value)}>
			<For each={props.options}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
		</select>
		<Show when={props.hint}>
			<p class={s.hint} style={props.hintStyle}>
				{props.hint}
			</p>
		</Show>
	</div>
);

export const SettingSlider: Component<{
	label: string;
	value: number;
	onChange: (value: number) => void;
	/** Fires once when the drag is released (DOM `change`), e.g. to play a preview at the committed value */
	onCommit?: (value: number) => void;
	min: number;
	max: number;
	step?: number;
	suffix?: string;
	formatValue?: (value: number) => string;
	hint?: string;
}> = (props) => (
	<div class={s.group} id={settingSlugId(props.label)}>
		<label>{props.label}</label>
		<div class={s.slider}>
			<input
				type="range"
				min={props.min}
				max={props.max}
				step={props.step}
				value={props.value}
				onInput={(e) => props.onChange(parseInt(e.currentTarget.value, 10))}
				onChange={(e) => props.onCommit?.(parseInt(e.currentTarget.value, 10))}
			/>
			<span>{props.formatValue ? props.formatValue(props.value) : `${props.value}${props.suffix ?? ""}`}</span>
		</div>
		<Show when={props.hint}>
			<p class={s.hint}>{props.hint}</p>
		</Show>
	</div>
);

export const SettingInput: Component<{
	label: string;
	value: string;
	onInput: (value: string) => void;
	placeholder?: string;
	hint?: string;
	type?: "text" | "password" | "number";
}> = (props) => (
	<div class={s.group} id={settingSlugId(props.label)}>
		<label>{props.label}</label>
		<input
			type={props.type ?? "text"}
			value={props.value}
			onInput={(e) => props.onInput(e.currentTarget.value)}
			placeholder={props.placeholder}
		/>
		<Show when={props.hint}>
			<p class={s.hint}>{props.hint}</p>
		</Show>
	</div>
);
