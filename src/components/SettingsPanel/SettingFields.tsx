import { type Component, For, type JSX, Show } from "solid-js";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { ContextualHelp } from "./ContextualHelp";
import s from "./Settings.module.css";
import { SettingRow } from "./SettingsPrimitives";

export { SettingAccount, type SettingAccountOption, type SettingAccountProps } from "./fields/SettingAccount";
export { SettingButton, type SettingButtonProps } from "./fields/SettingButton";
export { SettingList, type SettingListProps, type SettingListRow } from "./fields/SettingList";
export { SettingMultiSelect, type SettingMultiSelectProps } from "./fields/SettingMultiSelect";
export { SettingStatus, type SettingStatusProps } from "./fields/SettingStatus";

export const SettingToggle: Component<{
	checked: boolean;
	onChange: (checked: boolean) => void;
	label: string;
	row?: boolean;
	hint?: string;
	hintStyle?: JSX.CSSProperties;
}> = (props) => (
	<Show
		when={props.row}
		fallback={
			<div class={s.group}>
				<div class={s.toggle}>
					<input type="checkbox" checked={props.checked} onChange={(e) => props.onChange(e.currentTarget.checked)} />
					<span>{props.label}</span>
					<ContextualHelp label={props.label} hint={props.hint} />
				</div>
				<Show when={props.hint}>
					<p class={s.hint} style={props.hintStyle}>
						{props.hint}
					</p>
				</Show>
			</div>
		}
	>
		<SettingRow label={props.label} hint={props.hint}>
			{(id, descriptionId) => (
				<input
					id={id}
					aria-describedby={descriptionId()}
					type="checkbox"
					class={s.rowSwitch}
					role="switch"
					checked={props.checked}
					onChange={(e) => props.onChange(e.currentTarget.checked)}
				/>
			)}
		</SettingRow>
	</Show>
);

export const SettingSelect: Component<{
	label: string;
	row?: boolean;
	value: string;
	onChange: (value: string) => void;
	options: { value: string; label: string }[];
	hint?: string;
	hintStyle?: JSX.CSSProperties;
}> = (props) => (
	<Show
		when={props.row}
		fallback={
			<div class={s.group}>
				<label>{props.label}</label>
				<ContextualHelp label={props.label} hint={props.hint} />
				<Select value={props.value} onChange={(e) => props.onChange(e.currentTarget.value)}>
					<For each={props.options}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
				</Select>
				<Show when={props.hint}>
					<p class={s.hint} style={props.hintStyle}>
						{props.hint}
					</p>
				</Show>
			</div>
		}
	>
		<SettingRow label={props.label} hint={props.hint}>
			{(id, descriptionId) => (
				<Select
					id={id}
					aria-describedby={descriptionId()}
					value={props.value}
					onChange={(e) => props.onChange(e.currentTarget.value)}
				>
					<For each={props.options}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
				</Select>
			)}
		</SettingRow>
	</Show>
);

export const SettingSlider: Component<{
	label: string;
	row?: boolean;
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
	<Show
		when={props.row}
		fallback={
			<div class={s.group}>
				<label>{props.label}</label>
				<ContextualHelp label={props.label} hint={props.hint} />
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
		}
	>
		<SettingRow label={props.label} hint={props.hint}>
			{(id, descriptionId) => (
				<div class={s.slider}>
					<input
						id={id}
						aria-describedby={descriptionId()}
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
			)}
		</SettingRow>
	</Show>
);

export const SettingInput: Component<{
	label: string;
	row?: boolean;
	value: string;
	onInput: (value: string) => unknown;
	/** Fires when the field loses focus, for checks that would reject a half-typed value. */
	onBlur?: () => void;
	placeholder?: string;
	hint?: string;
	type?: "text" | "password" | "number";
}> = (props) => (
	<Show
		when={props.row}
		fallback={
			<div class={s.group}>
				<label>{props.label}</label>
				<ContextualHelp label={props.label} hint={props.hint} />
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
		}
	>
		<SettingRow label={props.label} hint={props.hint}>
			{(id, descriptionId) => (
				<Input
					id={id}
					aria-describedby={descriptionId()}
					type={props.type ?? "text"}
					value={props.value}
					onInput={(e) => {
						if (props.onInput(e.currentTarget.value) === false) e.currentTarget.value = props.value;
					}}
					onBlur={() => props.onBlur?.()}
					placeholder={props.placeholder}
				/>
			)}
		</SettingRow>
	</Show>
);
