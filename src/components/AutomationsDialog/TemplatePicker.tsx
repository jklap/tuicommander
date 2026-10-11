import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { t } from "../../i18n";
import type { AutomationsStore } from "../../stores/automations";
import s from "./AutomationsDialog.module.css";
import type { AutomationTemplate } from "./contract";

export function TemplatePicker(props: { store: AutomationsStore }) {
	const [templates, setTemplates] = createSignal<AutomationTemplate[]>([]);
	const [selected, setSelected] = createSignal("");
	const [error, setError] = createSignal("");
	let disposed = false;
	onCleanup(() => {
		disposed = true;
	});
	const load = async () => {
		setError("");
		try {
			const values = await props.store.templates();
			if (!disposed) setTemplates(values);
		} catch (cause) {
			if (!disposed) setError(cause instanceof Error ? cause.message : String(cause));
		}
	};
	onMount(() => {
		void load();
	});
	return (
		<fieldset class={s.group} disabled={props.store.busy()}>
			<legend>{t("automations.templates", "Built-in templates")}</legend>
			<Show when={error()}>
				<p role="alert" class={s.error}>
					{error()}
				</p>
				<button type="button" onClick={() => void load()}>
					{t("automations.refresh", "Refresh")}
				</button>
			</Show>
			<label>
				{t("automations.template", "Template")}
				<select value={selected()} onChange={(event) => setSelected(event.currentTarget.value)}>
					<option value="">{t("automations.chooseTemplate", "Choose a template")}</option>
					<For each={templates()}>
						{(item) => <option value={item.id}>{t("automations.templateNames." + item.id, item.name)}</option>}
					</For>
				</select>
			</label>
			<p class={s.hint}>
				{t(
					"automations.templateHint",
					"Enter a repository and run config above, then apply a template. Review the draft and save it paused; Resume enables scheduled runs.",
				)}
			</p>
			<button
				type="button"
				disabled={!selected() || !props.store.draft()?.repository.trim() || !props.store.draft()?.run_config.trim()}
				onClick={() => void props.store.applyTemplate(selected())}
			>
				{t("automations.applyTemplate", "Apply template")}
			</button>
		</fieldset>
	);
}
