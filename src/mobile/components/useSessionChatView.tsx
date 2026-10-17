import { createEffect, createSignal, Show } from "solid-js";
import type { AgentType } from "../../agents";
import { TerminalChatTranscript } from "../../components/Terminal/TerminalChatTranscript";
import { type ChatViewWake, chatViewStore } from "../../stores/chatView";
import styles from "./SessionChatView.module.css";

/** Mode belongs to a PTY, not to its polled SessionInfo object. */
const modes = new Map<string, "cli" | "chat">();
export function useSessionChatView(props: {
	sessionId: () => string;
	agentType: () => string | undefined;
	sessionExists: () => boolean;
	subscribeWake: ChatViewWake;
}) {
	const [atBottom, setAtBottom] = createSignal(true);
	let jump: (() => void) | undefined;
	const [mode, setMode] = createSignal<"cli" | "chat">(modes.get(props.sessionId()) ?? "cli");
	createEffect(() => setMode(modes.get(props.sessionId()) ?? "cli"));
	const select = (next: "cli" | "chat") => {
		modes.set(props.sessionId(), next);
		setMode(next);
	};
	const available = () => (props.agentType() === "claude" || props.agentType() === "ego") && props.sessionExists();
	createEffect(() => {
		if (mode() === "chat" && !available()) {
			chatViewStore.markUnavailable(props.sessionId(), "No agent is running in this terminal");
			select("cli");
		}
	});
	const active = () => mode() === "chat" && available();
	const openChat = () => {
		chatViewStore.clearUnavailable(props.sessionId());
		select("chat");
	};
	const Toggle = () => (
		<Show when={available()}>
			<div class={styles.toggle} role="group" aria-label="Terminal view">
				<button type="button" aria-pressed={!active()} onClick={() => select("cli")}>
					CLI
				</button>
				<button type="button" aria-pressed={active()} onClick={openChat}>
					Chat
				</button>
			</div>
		</Show>
	);
	const View = () => (
		<>
			<Show when={!active() && chatViewStore.unavailableReason(props.sessionId())}>
				{(reason) => (
					<div class={styles.notice} role="status">
						Chat view closed: {reason()}
					</div>
				)}
			</Show>
			<Show when={active() && props.sessionId()} keyed>
				{(sessionId) => (
					<TerminalChatTranscript
						sessionId={sessionId}
						terminalAgent={props.agentType() as AgentType}
						mobile
						onAtBottomChange={setAtBottom}
						onRegisterScrollToBottom={(fn) => {
							jump = fn;
							setAtBottom(true);
						}}
						subscribeWake={props.subscribeWake}
						onUnavailable={() => select("cli")}
						hint="Reply below. Switch to CLI for permission prompts or questions missing here."
					/>
				)}
			</Show>
		</>
	);
	return { active, available, openChat, Toggle, View, atBottom, scrollToBottom: () => jump?.() };
}
