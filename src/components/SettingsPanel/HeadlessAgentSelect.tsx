import { type Component, createEffect, For, type JSX, Show, splitProps } from "solid-js";
import { AGENTS, type AgentType, toHeadlessAgentChoice } from "../../agents";
import { agentConfigsStore } from "../../stores/agentConfigs";
import { Select } from "../ui/Select";

/** SmartPromptsTab's "Headless Agent" select. Values go through
 * `toHeadlessAgentChoice`, so a composite `agentType:configName` run-config
 * choice persists and anything naming no known agent stores "not configured".
 *
 * `selected` (not the `<select>`'s own `value`) is bound per-option so a
 * late-arriving option — agent detection is async and starts empty — still
 * lands correctly once inserted. A top-level `value` is only applied once,
 * against whatever options exist at that instant; if detection hasn't
 * resolved yet, the browser silently falls back to selecting the first
 * option instead of the stored value, and nothing ever re-applies it.
 * The shared `ui/Select` is used without its `value` prop for that reason;
 * remaining attributes (id, aria-describedby) pass through.
 * `ui/Select` builds its options before attaching them, so a post-render
 * effect also re-applies the stored value whenever it or the option set
 * changes — effects run after the options are in the DOM. */
export const HeadlessAgentSelect: Component<
	{ agentTypes: AgentType[] } & Omit<JSX.SelectHTMLAttributes<HTMLSelectElement>, "value" | "onChange" | "children">
> = (props) => {
	const [local, rest] = splitProps(props, ["agentTypes"]);
	let element: HTMLSelectElement | undefined;
	createEffect(() => {
		const value = agentConfigsStore.getHeadlessAgent() ?? "";
		for (const type of local.agentTypes) agentConfigsStore.getRunConfigs(type);
		if (element) element.value = value;
	});
	return (
		<Select
			{...rest}
			data-setting-id="agents.headless_agent"
			ref={(el) => {
				element = el;
			}}
			onChange={(e) => {
				agentConfigsStore.setHeadlessAgent(toHeadlessAgentChoice(e.currentTarget.value));
			}}
		>
			<option value="" selected={!agentConfigsStore.getHeadlessAgent()}>
				— Not configured —
			</option>
			<For each={local.agentTypes}>
				{(type) => {
					const configs = () => agentConfigsStore.getRunConfigs(type);
					return (
						<Show
							when={configs().length > 0}
							fallback={
								<option value={type} selected={agentConfigsStore.getHeadlessAgent() === type}>
									{AGENTS[type]?.name ?? type}
								</option>
							}
						>
							<optgroup label={AGENTS[type]?.name ?? type}>
								<option value={type} selected={agentConfigsStore.getHeadlessAgent() === type}>
									{AGENTS[type]?.name ?? type} (default)
								</option>
								<For each={configs()}>
									{(cfg) => {
										const compositeValue = `${type}:${cfg.name}`;
										return (
											<option value={compositeValue} selected={agentConfigsStore.getHeadlessAgent() === compositeValue}>
												{cfg.name}
												{cfg.is_default ? " (default)" : ""}
											</option>
										);
									}}
								</For>
							</optgroup>
						</Show>
					);
				}}
			</For>
			<option value="api" selected={agentConfigsStore.getHeadlessAgent() === "api"}>
				API (via ego)
			</option>
		</Select>
	);
};
