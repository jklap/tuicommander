import { type Component, createEffect, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { chatViewAvailability, chatViewStore } from "../../stores/chatView";
import { terminalsStore } from "../../stores/terminals";
import { openFileLinkHere, resolveFileLink } from "../../utils/openFileLink";
import type { TranscriptSearchRef } from "../AIChatPanel/Transcript";
import { TerminalChatTranscript } from "./TerminalChatTranscript";
import s from "./TerminalChatView.module.css";

/**
 * Read-only transcript of a Claude terminal, shown in place of the grid.
 *
 * Mounted only while the terminal is in chat mode; the grid stays mounted and
 * hidden by the caller. Falls back to the grid on its own when the agent goes
 * away or the backend reports the terminal as not bound, and says why.
 */
export const TerminalChatView: Component<{
	terminalId: string;
	sessionId: string;
	onSearchRef?: (ref: TranscriptSearchRef | undefined) => void;
}> = (props) => {
	createEffect(() => {
		const availability = chatViewAvailability(terminalsStore.get(props.terminalId));
		const reason = availability.available ? chatViewStore.unavailableReason(props.sessionId) : availability.reason;
		if (reason === null) return;
		chatViewStore.markUnavailable(props.sessionId, reason);
		terminalsStore.setViewMode(props.terminalId, "cli");
	});

	/** False when the path resolves to nothing, so the caller can drop the dead link. */
	const openFile = async (href: string) => {
		const cwd = terminalsStore.get(props.terminalId)?.cwd;
		if (!cwd) return;
		try {
			const target = await resolveFileLink(cwd, href);
			if (!target) return false;
			openFileLinkHere(target);
		} catch (error) {
			appLogger.error("terminal", "Failed to resolve file link", error);
		}
	};

	return (
		<TerminalChatTranscript
			sessionId={props.sessionId}
			terminalAgent={terminalsStore.get(props.terminalId)?.agentType ?? undefined}
			onSearchRef={props.onSearchRef}
			onOpenFile={openFile}
			onUnavailable={() => terminalsStore.setViewMode(props.terminalId, "cli")}
		/>
	);
};

/** The CLI | Chat switch of a terminal that runs an agent. */
export const ViewModeToggle: Component<{ terminalId: string; onWheel?: (e: WheelEvent) => void }> = (props) => {
	const term = () => terminalsStore.get(props.terminalId);
	const availability = () => chatViewAvailability(term());
	const mode = () => term()?.viewMode ?? "cli";
	const reason = () => {
		const a = availability();
		return a.available ? undefined : a.reason;
	};
	return (
		<Show when={term()?.agentType}>
			<div class={s.toggle} data-mode={mode()} role="group" aria-label="Terminal view" onWheel={props.onWheel}>
				<button
					type="button"
					class={s.toggleButton}
					aria-pressed={mode() === "cli"}
					onClick={() => terminalsStore.setViewMode(props.terminalId, "cli")}
				>
					CLI
				</button>
				<button
					type="button"
					class={s.toggleButton}
					aria-pressed={mode() === "chat"}
					disabled={!availability().available}
					title={reason()}
					onClick={() => {
						const sessionId = term()?.sessionId;
						if (sessionId) chatViewStore.clearUnavailable(sessionId);
						terminalsStore.setViewMode(props.terminalId, "chat");
					}}
				>
					Chat
				</button>
			</div>
		</Show>
	);
};

/** One line left behind when the chat view closed itself. */
export const ChatViewFallbackBanner: Component<{ terminalId: string }> = (props) => {
	const term = () => terminalsStore.get(props.terminalId);
	const reason = () => {
		const sessionId = term()?.sessionId;
		return sessionId && term()?.viewMode === "cli" ? chatViewStore.unavailableReason(sessionId) : null;
	};
	return <Show when={reason()}>{(text) => <div class={s.fallbackBanner}>Chat view closed: {text()}</div>}</Show>;
};
