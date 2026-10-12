import { Select } from "../ui/Select";
import { Input } from "../ui/Input";
import { type Component, For, type JSX, Show } from "solid-js";
import s from "./Settings.module.css";

export { SettingAccount, type SettingAccountOption, type SettingAccountProps } from "./fields/SettingAccount";
export { SettingButton, type SettingButtonProps } from "./fields/SettingButton";
export { SettingList, type SettingListProps, type SettingListRow } from "./fields/SettingList";
export { SettingMultiSelect, type SettingMultiSelectProps } from "./fields/SettingMultiSelect";
export { SettingStatus, type SettingStatusProps } from "./fields/SettingStatus";

export const SettingToggle: Component<{
	checked: boolean;
	onChange: (checked: boolean) => void;
	label: string;
	hint?: string;
	hintStyle?: JSX.CSSProperties;
}> = (props) => (
	<div class={s.group}>
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
	<div class={s.group}>
		<label>{props.label}</label>
		<Select value={props.value} onChange={(e) => props.onChange(e.currentTarget.value)}>
			<For each={props.options}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
		</Select>
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
	<div class={s.group}>
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
	onInput: (value: string) => unknown;
	/** Fires when the field loses focus, for checks that would reject a half-typed value. */
	onBlur?: () => void;
	placeholder?: string;
	hint?: string;
	type?: "text" | "password" | "number";
}> = (props) => (
	<div class={s.group}>
		<label>{props.label}</label>
		<Input
			type={props.type ?? "text"}
			value={props.value}
			onInput={(e) => {
				if (props.onInput(e.currentTarget.value) === false) e.currentTarget.value = props.value;
			}}
			onBlur={() => props.onBlur?.()}
			placeholder={props.placeholder}
		/>
		<Show when={props.hint}>
			<p class={s.hint}>{props.hint}</p>
		</Show>
	</div>
);
