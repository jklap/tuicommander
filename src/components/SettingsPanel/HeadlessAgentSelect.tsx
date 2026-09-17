import { type Component, For, Show, splitProps } from "solid-js";
import { AGENTS, type AgentType, toHeadlessAgentChoice } from "../../agents";
import { agentConfigsStore } from "../../stores/agentConfigs";
import { cx } from "../../utils/cx";
import fc from "../ui/FormControls.module.css";
import type { SelectProps } from "../ui/Select";

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
 * The shared `Select` primitive is not used: it drives selection through
 * `value` (re-applied only when the value changes, not when options arrive),
 * and per-option `selected` does not survive its child insertion order. This
 * native select carries the same `data-form-control` styling instead. Other
 * select attributes (`id`, `aria-describedby`, `data-setting-id`) are
 * forwarded. */
export const HeadlessAgentSelect: Component<
	{ agentTypes: AgentType[] } & Omit<SelectProps, "value" | "onChange" | "children">
> = (props) => {
	const [local, rest] = splitProps(props, ["agentTypes", "class", "compact"]);
	return (
		<select
			{...rest}
			data-form-control
			class={cx(fc.control, fc.select, local.compact && fc.compact, local.class)}
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
		</select>
	);
};
