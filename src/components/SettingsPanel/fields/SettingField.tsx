import { type Component, createUniqueId, type JSX, Show } from "solid-js";
import s from "../Settings.module.css";

export interface SettingFieldProps {
	label: string;
	hint?: string;
	error?: string;
	row?: boolean;
	disabled?: boolean;
}

export const SettingField: Component<
	SettingFieldProps & {
		children: (id: string, hintId: string | undefined, descriptionId: () => string | undefined) => JSX.Element;
	}
> = (props) => {
	const id = createUniqueId();
	const descriptionId = () =>
		[props.hint && `${id}-hint`, props.error && `${id}-error`].filter(Boolean).join(" ") || undefined;
	return (
		<Show
			when={props.row}
			fallback={
				<div class={s.group}>
					<label for={id}>{props.label}</label>
					{props.children(id, descriptionId(), descriptionId)}
					<Show when={props.hint}>
						<p id={`${id}-hint`} class={s.hint}>
							{props.hint}
						</p>
					</Show>
					<Show when={props.error}>
						<p id={`${id}-error`} class={s.fieldError}>
							{props.error}
						</p>
					</Show>
				</div>
			}
		>
			<div class={s.settingRow}>
				<div class={s.settingRowText}>
					<label for={id}>{props.label}</label>
					<Show when={props.hint}>
						<p id={`${id}-hint`} class={s.hint}>
							{props.hint}
						</p>
					</Show>
				</div>
				<div class={s.settingRowControl}>
					{props.children(id, undefined, descriptionId)}
					<Show when={props.error}>
						<p id={`${id}-error`} class={s.fieldError}>
							{props.error}
						</p>
					</Show>
				</div>
			</div>
		</Show>
	);
};
