import type { Component } from "solid-js";
import type { WorkItemTone } from "../../../types/work-items.gen";
import s from "./fields.module.css";
import { SettingField, type SettingFieldProps } from "./SettingField";

export interface SettingStatusProps extends SettingFieldProps {
	value: string;
	tone?: WorkItemTone;
}

export const SettingStatus: Component<SettingStatusProps> = (props) => (
	<SettingField label={props.label} hint={props.hint}>
		{(id, hintId) => (
			<output
				id={id}
				aria-label={props.label}
				aria-describedby={hintId}
				class={s.status}
				data-tone={props.tone ?? "neutral"}
			>
				{props.value}
			</output>
		)}
	</SettingField>
);
