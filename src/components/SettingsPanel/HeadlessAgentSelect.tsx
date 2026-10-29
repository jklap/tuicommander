import { type Component, For, Show, splitProps } from "solid-js";
import { AGENTS, type AgentType, toHeadlessAgentChoice } from "../../agents";
import { agentConfigsStore } from "../../stores/agentConfigs";
import { Select, type SelectProps } from "../ui/Select";

/** SmartPromptsTab's "Headless Agent" select. Values go through
 * `toHeadlessAgentChoice`, so a composite `agentType:configName` run-config
 * choice persists and anything naming no known agent stores "not configured".
 *
 * Agent detection is async and starts empty, so the stored value may name an
 * option that does not exist yet. The shared `Select` re-applies `value` only
 * when the value expression re-runs, so `value` below also reads the option
 * set (agent types and their run configs): when a late option arrives, the
 * value is applied again against the options now present. Per-option
 * `selected` stays as a second line of defence. Other select attributes
 * (`id`, `aria-describedby`) are forwarded; the catalogue id is fixed here
 * because this control only ever edits `agents.headless_agent`. */
export const HeadlessAgentSelect: Component<
	{ agentTypes: AgentType[] } & Omit<SelectProps, "value" | "onChange" | "children">
> = (props) => {
	const [local, rest] = splitProps(props, ["agentTypes"]);
	const value = () => {
		for (const type of local.agentTypes) agentConfigsStore.getRunConfigs(type);
		return agentConfigsStore.getHeadlessAgent() ?? "";
	};
	return (
		<Select
			{...rest}
			data-setting-id="agents.headless_agent"
			value={value()}
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
