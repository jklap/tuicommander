import { createEffect, createSignal, For, onMount, Show } from "solid-js";
import { Composer } from "../../components/AIChatPanel/Composer";
import { Interactions } from "../../components/AIChatPanel/Interactions";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import { createAcpChat } from "../../components/AIChatPanel/useAcpChat";
import { invoke } from "../../invoke";
import { acpTranscript } from "../../stores/acpTranscript";
import { appLogger } from "../../stores/appLogger";
import { settingsStore } from "../../stores/settings";
import type { AcpHostRequestId } from "../../types/acp";
import styles from "./MobileChatScreen.module.css";

export function MobileChatScreen() {
	const linkedRepository = new URLSearchParams(location.search).get("repo");
	const linkedSession = new URLSearchParams(location.search).get("session");
	const [repositories, setRepositories] = createSignal<string[]>([]);
	const [root, setRoot] = createSignal<string | null>(null);
	const [repositoryError, setRepositoryError] = createSignal<string | null>(null);
	const chat = createAcpChat(root, () => true);
	const answering = new Set<AcpHostRequestId>();
	let linkHandled = false;

	createEffect(() => {
		if (linkHandled || !linkedRepository || !linkedSession || root() !== linkedRepository) return;
		if (chat.phase() !== "live" || !chat.sessions().some((session) => session.sessionId === linkedSession)) return;
		linkHandled = true;
		if (chat.sessionId() !== linkedSession) void chat.selectSession(linkedSession);
	});

	onMount(() => {
		void settingsStore.hydrate();
		void invoke<{ repos?: Record<string, unknown> }>("load_repositories")
			.then((config) => {
				const paths = Object.keys(config.repos ?? {});
				setRepositories(paths);
				setRoot(
					(current) =>
						current ?? (linkedRepository && paths.includes(linkedRepository) ? linkedRepository : paths[0]) ?? null,
				);
			})
			.catch((error: unknown) => {
				setRepositoryError("Could not load repositories.");
				appLogger.warn("ai-chat", "Could not load mobile chat repositories", error);
			});
	});

	async function answerOnce(requestId: AcpHostRequestId, action: () => Promise<void>): Promise<void> {
		if (answering.has(requestId)) return;
		answering.add(requestId);
		await action();
		if (chat.error()) answering.delete(requestId);
	}

	return (
		<section class={styles.screen} aria-label="AI Chat">
			<header class={styles.header}>
				<strong>AI Chat</strong>
				<Show when={repositories().length > 0}>
					<select aria-label="Repository" value={root() ?? ""} onChange={(event) => setRoot(event.currentTarget.value)}>
						<For each={repositories()}>
							{(path) => <option value={path}>{path.split(/[/\\]/).filter(Boolean).at(-1) ?? path}</option>}
						</For>
					</select>
				</Show>
			</header>
			<Show when={repositoryError()}>{(message) => <div class={styles.banner}>{message()}</div>}</Show>
			<Show when={chat.gap()}>
				{(gap) => (
					<div class={styles.banner}>
						Missed part of this conversation: {gap().message}{" "}
						<button type="button" onClick={() => void chat.recover()}>
							Recover
						</button>
					</div>
				)}
			</Show>
			<Show when={chat.error()}>
				{(message) => (
					<div class={styles.banner}>
						{message()}{" "}
						<button type="button" onClick={() => void chat.recover()}>
							Retry
						</button>
					</div>
				)}
			</Show>
			<Show when={chat.phase() === "live" && !chat.isStreaming()}>
				<div class={styles.banner}>Not receiving updates.</div>
			</Show>
			<Show when={chat.phase() === "live"}>
				<div class={styles.controls}>
					<label>
						Conversation
						<select
							aria-label="Conversation"
							value={chat.sessionId() ?? ""}
							onChange={(event) => void chat.selectSession(event.currentTarget.value)}
						>
							<For each={chat.sessions()}>
								{(session) => (
									<option value={session.sessionId}>
										{acpTranscript.title(session.sessionId) || session.title || session.sessionId}
									</option>
								)}
							</For>
						</select>
					</label>
					<button type="button" onClick={() => void chat.startSession()}>
						New
					</button>
				</div>
			</Show>
			<Transcript
				entries={chat.entries}
				busy={chat.busy}
				onSuggestion={(text) => void chat.send(text)}
				emptyMessage={
					chat.phase() === "unconfigured"
						? "Configure ego in desktop Settings to start a conversation."
						: root()
							? "Ask ego about this repository."
							: "Add a repository to start a conversation."
				}
			>
				<Interactions
					interactions={chat.interactions}
					onPermission={(requestId, optionId) =>
						void answerOnce(requestId, () => chat.answerPermission(requestId, optionId))
					}
					onPermissionDismissed={(requestId) => void answerOnce(requestId, () => chat.cancelPermission(requestId))}
					onElicitationAccepted={(requestId, content) =>
						void answerOnce(requestId, () => chat.answerElicitation(requestId, { action: "accept", content }))
					}
					onElicitationDeclined={(requestId) =>
						void answerOnce(requestId, () => chat.answerElicitation(requestId, { action: "decline" }))
					}
					onElicitationCancelled={(requestId) =>
						void answerOnce(requestId, () => chat.answerElicitation(requestId, { action: "cancel" }))
					}
				/>
			</Transcript>
			<Show when={chat.phase() === "live"}>
				<Composer chat={chat} />
			</Show>
		</section>
	);
}
