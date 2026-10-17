import { type Component, createEffect, createSignal, For, Show, untrack } from "solid-js";
import type { SettingId } from "../../../services/settingsCatalogue";
import { reportSettingsConflicts } from "../../../services/settingsRefresh";
import s from "../Settings.module.css";
import a from "./AgentsTab.module.css";

/** The marker tokens the TUIC parser needs that `text` no longer contains. */
export function missingMarkers(text: string, required: readonly string[]): string[] {
	return required.filter((marker) => !text.includes(marker));
}

/**
 * Edits launch instructions that TUIC passes to an agent at launch. `value`
 * undefined means "no override": the editor shows `fallback` and Reset hides.
 * Saves on blur, so a half-typed edit never reaches a launch.
 */
export const LaunchInstructionsEditor: Component<{
	id: string;
	settingId: SettingId;
	label: string;
	value: string | undefined;
	fallback: string;
	requiredMarkers: readonly string[];
	onSave: (text: string | undefined) => void;
}> = (props) => {
	const [draft, setDraft] = createSignal(props.value ?? props.fallback);
	let acknowledged = props.value ?? props.fallback;
	createEffect(() => {
		const incoming = props.value ?? props.fallback;
		const current = untrack(draft);
		if (current === acknowledged) setDraft(incoming);
		else if (incoming !== acknowledged && incoming !== current) reportSettingsConflicts(["/"]);
		acknowledged = incoming;
	});
	const missing = () => (draft().trim() === "" ? [] : missingMarkers(draft(), props.requiredMarkers));
	const save = () => {
		if (draft() !== (props.value ?? props.fallback)) props.onSave(draft());
	};
	return (
		<div class={a.expandedSection}>
			<label class={a.expandedLabel} for={props.id}>
				{props.label}
			</label>
			<textarea
				data-setting-id={props.settingId}
				id={props.id}
				class={`${a.formInput} ${a.mono}`}
				rows={10}
				value={draft()}
				onInput={(e) => setDraft(e.currentTarget.value)}
				onBlur={save}
				onClick={(e) => e.stopPropagation()}
			/>
			<Show when={missing().length > 0}>
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
				<button
					data-setting-id={props.settingId}
					type="button"
					class={a.smallBtn}
					onClick={() => {
						setDraft(props.fallback);
						props.onSave(undefined);
					}}
				>
					Reset to default
				</button>
			</Show>
			<p class={s.hint}>
				Passed to the agent at every launch and resume. Empty turns it off. <code>{"{VER}"}</code> becomes the TUIC
				version.
			</p>
		</div>
	);
};
