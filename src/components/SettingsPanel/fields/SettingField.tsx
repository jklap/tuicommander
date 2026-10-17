import { type Component, createUniqueId, type JSX, Show } from "solid-js";
import { ContextualHelp } from "../ContextualHelp";
import s from "../Settings.module.css";

export interface SettingFieldProps {
	label: string;
	hint?: string;
	error?: string;
	help?: JSX.Element;
	actions?: JSX.Element;
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
					<div class={s.helpLabel}>
						<label for={id}>{props.label}</label>
						{props.help || <ContextualHelp label={props.label} hint={props.hint} />}
					</div>
					{props.children(id, descriptionId(), descriptionId)}
					<Show when={props.hint}>
						<p id={`${id}-hint`} class={s.hint}>
							{props.hint}
						</p>
					</Show>
					<Show when={props.error}>
						<p id={`${id}-error`} class={s.fieldError} role="alert">
							{props.error}
						</p>
					</Show>
				</div>
			}
		>
			<div class={s.settingRow}>
				<div class={s.settingRowText}>
					<div class={s.helpLabel}>
						<label for={id}>{props.label}</label>
						{props.help || <ContextualHelp label={props.label} hint={props.hint} />}
					</div>
					<Show when={props.hint}>
						<p id={`${id}-hint`} class={s.hint}>
							{props.hint}
						</p>
					</Show>
				</div>
				<div class={s.settingRowControl} classList={{ [s.settingRowWithActions]: !!props.actions }}>
					<Show when={props.actions} fallback={props.children(id, undefined, descriptionId)}>
						<div class={s.settingRowValue}>{props.children(id, undefined, descriptionId)}</div>
						{props.actions}
					</Show>
					<Show when={props.error}>
						<p id={`${id}-error`} class={s.fieldError} role="alert">
							{props.error}
						</p>
					</Show>
				</div>
			</div>
		</Show>
	);
};
