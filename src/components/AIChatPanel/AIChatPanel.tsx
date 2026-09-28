import { type Component, createMemo, Show } from "solid-js";
import { cx } from "../../utils";
import p from "../shared/panel.module.css";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { PanelWindowControls } from "../ui/PanelWindowControls";
import s from "./AIChatPanel.module.css";
import { Composer } from "./Composer";
import { Interactions } from "./Interactions";
import { SessionControls } from "./SessionControls";
import { Transcript } from "./Transcript";
import { createAcpChat } from "./useAcpChat";

const isPanelMode = () => new URLSearchParams(window.location.search).get("mode") === "panel";

/** The last segment of a path, for the header. */
function basename(path: string): string {
	const parts = path.split(/[/\\]/).filter(Boolean);
	return parts.at(-1) ?? path;
}

export interface AIChatPanelProps {
	visible: boolean;
	onClose: () => void;
	/** The repository this conversation is about. */
	repoPath: string | null;
	/** Effective filesystem root — the worktree path when on a linked worktree. */
	fsRoot?: string | null;
}

/**
 * AI Chat, running on ego over ACP.
 *
 * The panel binds to a repository and a session. It does not bind to a
 * terminal, and there is no per-terminal lock: a turn ego runs outlives any tab,
 * may touch files no tab is showing, and is the same conversation for every
 * window looking at that repository. The panel is a control plane over an agent
 * that lives outside it — see `docs/user-guide/ai-chat.md`.
 *
 * Nothing in here interprets a frame or holds a cursor. `acpTranscript` folds
 * the stream into something a person reads, `acpStore` holds what is true about
 * the connection, and `createAcpChat` owns which of those this panel is looking
 * at.
 */
export const AIChatPanel: Component<AIChatPanelProps> = (props) => {
	// The worktree path where there is one: ego works on the files the user is
	// looking at, not on the repository's main checkout.
	const root = createMemo(() => props.fsRoot || props.repoPath || null);
	const chat = createAcpChat(root, () => props.visible);

	const emptyMessage = () => {
		switch (chat.phase()) {
			case "unconfigured":
				return "ACP is not configured. Set the ego executable in Settings to start a conversation.";
			case "no-repo":
				return "Open a repository to start a conversation.";
			case "starting":
				return "Starting ego…";
			default:
				return "Ask ego about this repository.";
		}
	};

	return (
		<div id="ai-chat-panel" class={cx(s.panel, !props.visible && s.hidden)}>
			<PanelResizeHandle panelId="ai-chat-panel" minWidth={300} maxWidth={700} />

			<div class={p.header}>
				<div class={p.headerLeft}>
					<span class={p.title}>
						<svg
							width="14"
							height="14"
							viewBox="0 0 14 14"
							fill="currentColor"
							style={{ "vertical-align": "-2px", "margin-right": "4px" }}
						>
							<path
								d="M2 2.5A1.5 1.5 0 013.5 1h7A1.5 1.5 0 0112 2.5v6A1.5 1.5 0 0110.5 10H5l-3 2.5V10A1.5 1.5 0 010.5 8.5v-6z"
								transform="translate(1 0.5)"
							/>
						</svg>
						AI Chat
					</span>
					<Show when={root()}>{(path) => <span class={s.terminalName}>{basename(path())}</span>}</Show>
					<Show when={chat.title()}>{(title) => <span class={s.terminalName}>{title()}</span>}</Show>
				</div>
				<div class={s.headerActions}>
					<PanelWindowControls
						panelId="ai-chat"
						mode={isPanelMode() ? "detached" : "inline"}
						onInlineClose={props.onClose}
					/>
				</div>
			</div>

			{/* A gap is not a transport hiccup: the journal no longer holds the
			    sequence this window asked for, so the conversation on screen has a
			    hole in it. Saying so and offering the one recovery there is — a
			    fresh process replaying the history — is the whole point of
			    surfacing it rather than skipping ahead in silence. */}
			<Show when={chat.gap()}>
				{(gap) => (
					<div class={s.errorBanner}>
						<span class={s.errorText}>Missed part of this conversation: {gap().message}</span>
						<button type="button" class={s.retryBtn} onClick={() => void chat.recover()}>
							Recover
						</button>
					</div>
				)}
			</Show>

			<Show when={chat.error()}>
				{(message) => (
					<div class={s.errorBanner}>
						<span class={s.errorText}>{message()}</span>
						<button type="button" class={s.retryBtn} onClick={() => void chat.recover()}>
							Retry
						</button>
					</div>
				)}
			</Show>

			{/* A connection that is up but has nobody reading its journal is not a
			    live panel. Say so rather than showing a conversation that has
			    quietly stopped moving. */}
			<Show when={chat.phase() === "live" && !chat.isStreaming()}>
				<div class={s.frozenBanner}>Not receiving updates.</div>
			</Show>

			<Show when={chat.phase() === "live"}>
				<SessionControls chat={chat} />
			</Show>

			<Transcript entries={chat.entries} busy={chat.busy} emptyMessage={emptyMessage()}>
				<Interactions
					interactions={chat.interactions}
					onPermission={(requestId, optionId) => void chat.answerPermission(requestId, optionId)}
					onPermissionDismissed={(requestId) => void chat.cancelPermission(requestId)}
					onElicitationAccepted={(requestId, content) =>
						void chat.answerElicitation(requestId, { action: "accept", content })
					}
					onElicitationDeclined={(requestId) => void chat.answerElicitation(requestId, { action: "decline" })}
					onElicitationCancelled={(requestId) => void chat.answerElicitation(requestId, { action: "cancel" })}
				/>
			</Transcript>

			<Show when={chat.phase() === "live"}>
				<Composer chat={chat} />
			</Show>
			<Show when={chat.usage()}>
				{(usage) => (
					<div class={s.usageFooter}>
						<span>Context {Math.round((usage().used / usage().size) * 100)}%</span>
						<Show when={usage().cost}>
							{(cost) => (
								<span>
									{cost().currency} {cost().amount}
								</span>
							)}
						</Show>
					</div>
				)}
			</Show>
		</div>
	);
};

export default AIChatPanel;
