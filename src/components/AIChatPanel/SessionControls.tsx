/**
 * The knobs a session publishes, and the three verbs ego adds to a turn.
 *
 * Every option drawn here comes from the session itself — model, reasoning
 * effort, mode, whatever else it chose to publish. There is no list of models
 * in this file on purpose: a hardcoded one would be a second, wrong answer to a
 * question the session already answers, and it would go stale the first time
 * ego learned a new model.
 *
 * Pause, resume and compact are ego's own extensions. Each is drawn only when
 * the agent advertised it, so a build of ego without them shows no button
 * rather than a button that fails.
 */

import { type Component, For, Show } from "solid-js";
import type {
	AcpSessionConfigOption,
	AcpSessionConfigSelectGroup,
	AcpSessionConfigSelectOption,
} from "../../types/acp";
import { cx } from "../../utils";
import s from "./AIChatPanel.module.css";
import type { AcpChat } from "./useAcpChat";

type SelectOption = Extract<AcpSessionConfigOption, { type: "select" }>;

function isGrouped(options: SelectOption["options"]): options is AcpSessionConfigSelectGroup[] {
	return options.length > 0 && !("id" in options[0]);
}

function groups(option: SelectOption): AcpSessionConfigSelectGroup[] {
	return isGrouped(option.options) ? option.options : [];
}

function flat(option: SelectOption): AcpSessionConfigSelectOption[] {
	return isGrouped(option.options) ? [] : option.options;
}

const ConfigSelect: Component<{ option: SelectOption; chat: AcpChat }> = (props) => (
	<select
		class={s.modelPicker}
		title={props.option.description ?? props.option.name}
		value={props.option.currentValue}
		disabled={props.chat.busy()}
		onChange={(event) => void props.chat.setOption(props.option.id, { value: event.currentTarget.value })}
	>
		<For each={flat(props.option)}>{(choice) => <option value={choice.id}>{choice.name}</option>}</For>
		<For each={groups(props.option)}>
			{(group) => (
				<optgroup label={group.name}>
					<For each={group.options}>{(choice) => <option value={choice.id}>{choice.name}</option>}</For>
				</optgroup>
			)}
		</For>
	</select>
);

export const SessionControls: Component<{ chat: AcpChat }> = (props) => {
	const holdOffered = () => props.chat.capabilities()?.egoHoldVersion != null;
	const compactOffered = () => props.chat.capabilities()?.egoCompactVersion != null;
	const sessions = () => props.chat.sessions();

	return (
		<div class={s.controlBar}>
			{/* A boolean option is deliberately absent: the client does not advertise
			    `clientBooleanConfig`, so one cannot arrive, and drawing a control for
			    it would offer a switch the agent never said it would read. */}
			<For each={props.chat.configOptions()}>
				{(option) => (
					<Show when={option.type === "select" && (option as SelectOption)}>
						{(select) => <ConfigSelect option={select()} chat={props.chat} />}
					</Show>
				)}
			</For>

			<Show when={sessions().length > 1}>
				<select
					class={s.modelPicker}
					title="Conversation"
					value={props.chat.sessionId() ?? ""}
					onChange={(event) => props.chat.selectSession(event.currentTarget.value)}
				>
					<For each={sessions()}>
						{(attachment, index) => <option value={attachment.sessionId}>{`Conversation ${index() + 1}`}</option>}
					</For>
				</select>
			</Show>

			<span class={s.controlSpacer} />

			<Show when={holdOffered()}>
				<Show
					when={props.chat.held()}
					fallback={
						<button
							type="button"
							class={s.headerBtn}
							title="Pause the turn"
							disabled={!props.chat.busy()}
							onClick={() => void props.chat.pause()}
						>
							Pause
						</button>
					}
				>
					<button
						type="button"
						class={cx(s.headerBtn, s.headerBtnActive)}
						title="Resume the turn"
						onClick={() => void props.chat.resume()}
					>
						Resume
					</button>
				</Show>
			</Show>

			<Show when={compactOffered()}>
				<button
					type="button"
					class={s.headerBtn}
					title="Compact the conversation"
					disabled={props.chat.busy()}
					onClick={() => void props.chat.compact()}
				>
					Compact
				</button>
			</Show>

			<button
				type="button"
				class={s.headerBtn}
				title="Start another conversation on this repository"
				onClick={() => void props.chat.startSession()}
			>
				New
			</button>
		</div>
	);
};
