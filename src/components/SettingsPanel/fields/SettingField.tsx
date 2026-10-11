import { type Component, createUniqueId, type JSX, Show } from "solid-js";
import s from "../Settings.module.css";

export interface SettingFieldProps {
	label: string;
	hint?: string;
	disabled?: boolean;
}

export const SettingField: Component<
	SettingFieldProps & {
		children: (id: string, hintId: string | undefined) => JSX.Element;
	}
> = (props) => {
	const id = createUniqueId();
	return (
		<div class={s.group}>
			<label for={id}>{props.label}</label>
			{props.children(id, props.hint ? `${id}-hint` : undefined)}
			<Show when={props.hint}>
				<p id={`${id}-hint`} class={s.hint}>
					{props.hint}
				</p>
			</Show>
		</div>
	);
};
