import { type Component, createEffect, onCleanup, onMount } from "solid-js";
import type { AgentType } from "../../agents";
import { listen } from "../../invoke";
import { createVisibilityInterval } from "../../mobile/utils/visibilityInterval";
import { acpTranscript } from "../../stores/acpTranscript";
import { type ChatViewWake, chatViewKey, chatViewStore } from "../../stores/chatView";
import { settingsStore } from "../../stores/settings";
import { Transcript, type TranscriptSearchRef } from "../AIChatPanel/Transcript";
import s from "./TerminalChatView.module.css";

/** Shared lifecycle and renderer; hosts own mode, input and navigation. */
export const TerminalChatTranscript: Component<{
	sessionId: string;
	terminalAgent?: AgentType;
	mobile?: boolean;
	hint: string;
	subscribeWake?: ChatViewWake;
	onUnavailable: (reason: string) => void;
	onSearchRef?: (ref: TranscriptSearchRef | undefined) => void;
	onOpenFile?: (href: string) => void | Promise<boolean | undefined> | Promise<void>;
}> = (props) => {
	let disposed = false;
	let ready = false;
	let unwatch: (() => void) | undefined;
	const refresh = () => {
		if (ready && !disposed && document.visibilityState !== "hidden") {
			void chatViewStore.refresh(props.sessionId).catch(() => {});
		}
	};
	createEffect(() => {
		const reason = chatViewStore.unavailableReason(props.sessionId);
		if (reason !== null) props.onUnavailable(reason);
	});
	createVisibilityInterval(refresh, 5_000);
	onMount(() => {
		// Even injected wakes pass through visibility gating: SSE may remain live while hidden.
		const subscribe: ChatViewWake = (id, _wake) =>
			props.subscribeWake
				? props.subscribeWake(id, refresh)
				: listen<{ session_id: string }>("chat-view-changed", (event) => {
						if (event.payload.session_id === id) refresh();
					});
		void chatViewStore.watch(props.sessionId, subscribe).then((dispose) => {
			if (disposed) {
				dispose();
				return;
			}
			unwatch = dispose;
			ready = true;
			refresh();
		});
	});
	onCleanup(() => {
		disposed = true;
		unwatch?.();
	});
	// DEFERRED (2026-10-14) — 2000-entry mobile-viewport resets took 10.9–12.8s on
	// a loaded Mac. Measure on iPhone before choosing virtualization or a render cap.
	return (
		<div class={s.chatView} data-testid="terminal-chat-view">
			<Transcript
				terminalAgent={props.terminalAgent}
				mobile={props.mobile}
				onSearchRef={props.onSearchRef}
				onOpenFile={props.onOpenFile}
				entries={() => acpTranscript.entries(chatViewKey(props.sessionId))}
				busy={() => false}
				observeToolDuration={false}
				muteIntermediate={settingsStore.state.chatMuteIntermediateText}
				emptyMessage="No messages in this conversation yet."
			/>
			<div class={s.hint}>{props.hint}</div>
		</div>
	);
};
