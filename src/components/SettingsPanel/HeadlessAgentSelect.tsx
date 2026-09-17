import { type Component, For, Show } from "solid-js";
import { AGENTS, type AgentType } from "../../agents";
import { agentConfigsStore } from "../../stores/agentConfigs";

/** Shared by ProvidersTab and SmartPromptsTab's "Headless Agent" selects —
 * previously near-identical copies in both files, which is exactly how the
 * SmartPromptsTab copy grew its own composite-value guard bug independently
 * of ProvidersTab's (already-correct) version.
 *
 * `selected` (not the `<select>`'s own `value`) is bound per-option so a
 * late-arriving option — agent detection is async and starts empty — still
 * lands correctly once inserted. A top-level `value` is only applied once,
 * against whatever options exist at that instant; if detection hasn't
 * resolved yet, the browser silently falls back to selecting the first
 * option instead of the stored value, and nothing ever re-applies it. */
export const HeadlessAgentSelect: Component<{ agentTypes: AgentType[] }> = (props) => {
	return (
		<select
			onChange={(e) => {
				const val = e.currentTarget.value;
				agentConfigsStore.setHeadlessAgent(val || null);
			}}
		>
			<option value="" selected={!agentConfigsStore.getHeadlessAgent()}>
				— Not configured —
			</option>
			<For each={props.agentTypes}>
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
				External API
			</option>
		</select>
	);
};
