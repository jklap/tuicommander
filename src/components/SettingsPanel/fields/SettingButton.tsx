import type { Component } from "solid-js";
import s from "./fields.module.css";
import { SettingField, type SettingFieldProps } from "./SettingField";

export interface SettingButtonProps extends SettingFieldProps {
	value: string;
	onClick: () => void;
}

export const SettingButton: Component<SettingButtonProps> = (props) => (
	<SettingField label={props.label} hint={props.hint}>
		{(id, hintId) => (
			<button
				id={id}
				type="button"
				class={s.button}
				aria-label={props.value}
				aria-describedby={hintId}
				disabled={props.disabled}
				onClick={props.onClick}
			>
				{props.value}
			</button>
		)}
	</SettingField>
);
