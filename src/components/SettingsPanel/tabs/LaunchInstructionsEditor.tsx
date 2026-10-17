import { type Component, createEffect, createSignal, Show } from "solid-js";
import s from "../Settings.module.css";
import a from "./AgentsTab.module.css";

/** The marker tokens the TUIC parser needs that `text` no longer contains. */
export function missingMarkers(text: string, required: readonly string[]): string[] {
	return required.filter((marker) => !text.includes(marker));
}

/**
 * Edits user additions to launch instructions that TUIC passes to an agent at launch. `value`
 * undefined means "no override": show a read-only effective preview until Add instructions.
 * Saves on blur, so a half-typed edit never reaches a launch.
 */
export const LaunchInstructionsEditor: Component<{
	id: string;
	label: string;
	value: string | undefined;
	fallback: string | undefined;
	inheritanceLabel: string;
	customizeLabel: string;
	resetLabel: string;
	requiredMarkers: readonly string[];
	onSave: (text: string | undefined) => void;
}> = (props) => {
	const [draft, setDraft] = createSignal(props.value ?? "");
	createEffect(() => setDraft(props.value ?? ""));
	const save = () => {
		if (draft() !== (props.value ?? "")) props.onSave(draft());
	};
	return (
		<div class={a.expandedSection}>
			<label class={a.expandedLabel} for={props.id}>
				{props.label}
			</label>
			<Show
				when={props.value !== undefined}
				fallback={
					<>
						<p class={s.hint}>{props.inheritanceLabel}</p>
						<pre id={props.id} class={a.instructionsPreview} aria-label={props.label}>
							{props.fallback ?? "Loading preview…"}
						</pre>
						<button
							type="button"
							class={a.smallBtn}
							disabled={props.fallback === undefined}
							onClick={() => props.onSave("")}
						>
							{props.customizeLabel}
						</button>
					</>
				}
			>
				<p class={s.hint}>User additions only — enabled modules and global additions still apply.</p>
				<textarea
					id={props.id}
					class={`${a.formInput} ${a.mono}`}
					rows={10}
					value={draft()}
					onInput={(e) => setDraft(e.currentTarget.value)}
					onBlur={save}
					onClick={(e) => e.stopPropagation()}
				/>
			</Show>
			<Show when={props.value !== undefined}>
				<button type="button" class={a.smallBtn} onClick={() => props.onSave(undefined)}>
					{props.resetLabel}
				</button>
			</Show>
			<Show when={props.value !== undefined}>
				<pre class={a.instructionsPreview} aria-label={`${props.label} effective preview`}>
					{props.fallback ?? "Loading preview…"}
				</pre>
			</Show>
			<p class={s.hint}>
				Appended after generated modules at every launch and resume. Empty adds nothing. <code>{"{VER}"}</code> becomes
				the TUIC version.
			</p>
		</div>
	);
};
