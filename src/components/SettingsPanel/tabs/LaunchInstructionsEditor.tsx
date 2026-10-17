import { type Component, createEffect, createSignal, For, Show } from "solid-js";
import s from "../Settings.module.css";
import a from "./AgentsTab.module.css";

/** The marker tokens the TUIC parser needs that `text` no longer contains. */
export function missingMarkers(text: string, required: readonly string[]): string[] {
	return required.filter((marker) => !text.includes(marker));
}

/**
 * Edits launch instructions that TUIC passes to an agent at launch. `value`
 * undefined means "no override": show a read-only preview until Customize.
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
	const missing = () => (draft().trim() === "" ? [] : missingMarkers(draft(), props.requiredMarkers));
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
							onClick={() => props.onSave(props.fallback)}
						>
							{props.customizeLabel}
						</button>
					</>
				}
			>
				<p class={s.hint}>Own copy — changes to inherited instructions do not apply.</p>
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
			<Show when={props.value !== undefined && missing().length > 0}>
				<p class={s.warning} role="alert">
					Missing required marker{missing().length > 1 ? "s" : ""}:{" "}
					<For each={missing()}>
						{(marker, i) => (
							<>
								<code>{marker.trim()}</code>
								{i() < missing().length - 1 ? ", " : ""}
							</>
						)}
					</For>
					. TUIC parses these lines for the tab title, the suggest buttons and the answers view.
				</p>
			</Show>
			<Show when={props.value !== undefined}>
				<button type="button" class={a.smallBtn} onClick={() => props.onSave(undefined)}>
					{props.resetLabel}
				</button>
			</Show>
			<p class={s.hint}>
				Passed to the agent at every launch and resume. Empty turns it off. <code>{"{VER}"}</code> becomes the TUIC
				version.
			</p>
		</div>
	);
};
