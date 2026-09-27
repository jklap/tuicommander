/**
 * One repo root, one ego, one conversation — and the rules for moving between
 * them.
 *
 * The panel binds to a repository and a session, never to a terminal. A turn
 * ego runs is not a thing the focused tab owns: it outlives the tab, it may
 * touch files no tab is showing, and two windows looking at the same repo are
 * looking at the same conversation. An earlier build enforced a per-terminal
 * lock, which is the exact inverse of a control plane.
 *
 * Switching repository opens a new session and leaves the previous one running.
 * `bindings` is what makes that cheap: a root that already has a connection is
 * taken back as it was, so moving away and back costs nothing and never
 * launches a second ego on a root that already has one.
 */

import { createEffect, createSignal } from "solid-js";
import { invoke } from "../../invoke";
import { type AcpListedSession, acpClient } from "../../services/acpClient";
import { acpStore } from "../../stores/acp";
import { type AcpTranscriptEntry, acpTranscript } from "../../stores/acpTranscript";
import { appLogger } from "../../stores/appLogger";
import { settingsStore } from "../../stores/settings";
import type {
	AcpAttachmentSnapshot,
	AcpClientError,
	AcpConnectionId,
	AcpConnectionSnapshot,
	AcpContentBlock,
	AcpElicitationAction,
	AcpHostRequestId,
	AcpPendingInteraction,
	AcpSessionConfigOption,
	AcpSessionConfigOptionValue,
	AcpSessionId,
} from "../../types/acp";
import { updateAppConfig } from "../../utils/updateAppConfig";

/** What a root is currently using. Module scope, so a panel that unmounts and
 *  comes back finds its connection rather than starting another one. */
const bindings = new Map<string, { connectionId: AcpConnectionId; sessionId: AcpSessionId | null }>();

/** Roots with a connect in flight, so a second render cannot launch a second ego. */
const starting = new Set<string>();

/** Tests only: forget every root binding. */
export function resetAcpChatBindings(): void {
	bindings.clear();
	starting.clear();
}

export type AcpChatPhase = "unconfigured" | "no-repo" | "starting" | "failed" | "live";

/** The client surface this hook drives, named so a test can hand it another. */
export type AcpChatClient = Pick<
	typeof acpClient,
	| "connect"
	| "reconnect"
	| "disconnect"
	| "newSession"
	| "loadSession"
	| "listSessions"
	| "prompt"
	| "cancel"
	| "answerPermission"
	| "cancelPermission"
	| "answerElicitation"
	| "setConfigOption"
	| "pause"
	| "resumeTurn"
	| "compact"
>;

/** A refusal, as a person reads it. Rust answers with an `AcpClientError`. */
function describe(error: unknown): string {
	if (typeof error === "string") return error;
	if (error && typeof error === "object" && "message" in error) {
		return String((error as Partial<AcpClientError>).message);
	}
	return String(error);
}

export function createAcpChat(root: () => string | null, active: () => boolean, client: AcpChatClient = acpClient) {
	const [connectionId, setConnectionId] = createSignal<AcpConnectionId | null>(null);
	const [sessionId, setSessionId] = createSignal<AcpSessionId | null>(null);
	const [connecting, setConnecting] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const [listedSessions, setListedSessions] = createSignal<AcpListedSession[]>([]);

	async function remember(target: string, session: AcpSessionId): Promise<void> {
		await updateAppConfig<{ ai_chat_sessions?: Record<string, string> }>((config) => {
			config.ai_chat_sessions = { ...config.ai_chat_sessions, [target]: session };
		});
	}

	async function refreshSessions(id: AcpConnectionId, target: string): Promise<void> {
		const rows: AcpListedSession[] = [];
		let cursor: string | undefined;
		do {
			const page = await client.listSessions(id, target, cursor);
			rows.push(...page.sessions.filter((session) => session.cwd === target));
			cursor = page.nextCursor || undefined;
		} while (cursor);
		if (root() === target) {
			setListedSessions(rows.sort((a, b) => (b.updatedAt ?? "").localeCompare(a.updatedAt ?? "")));
		}
	}

	/** Run one action, holding what it refused rather than throwing at the panel. */
	async function guard<T>(what: string, action: () => Promise<T>): Promise<T | null> {
		try {
			const result = await action();
			setError(null);
			return result;
		} catch (failure) {
			appLogger.error("ai-chat", `${what} failed`, failure);
			setError(describe(failure));
			return null;
		}
	}

	async function open(target: string): Promise<void> {
		if (starting.has(target)) return;
		starting.add(target);
		setConnecting(true);
		try {
			const snapshot = await guard("connecting to ego", () => client.connect(target));
			if (!snapshot) return;
			const binding = { connectionId: snapshot.connectionId, sessionId: null as AcpSessionId | null };
			bindings.set(target, binding);
			// The signals move only while this root is still the one on screen: a
			// person who switched repositories mid-launch must not be shown the
			// session that finished opening behind them. The binding is recorded
			// either way, so going back finds it.
			if (root() === target) setConnectionId(snapshot.connectionId);
			if (snapshot.capabilities?.list)
				await guard("listing conversations", () => refreshSessions(snapshot.connectionId, target));
			const config = await guard("reading saved conversation", () =>
				invoke<{ ai_chat_sessions?: Record<string, string> }>("load_config"),
			);
			if (!config) return;
			const saved = config?.ai_chat_sessions?.[target];
			if (saved && snapshot.capabilities?.load) {
				binding.sessionId = saved;
				if (root() === target) setSessionId(saved);
				await guard("replaying the conversation", () => client.loadSession(snapshot.connectionId, saved, target));
				return;
			}
			const session = await guard("opening a session", () => client.newSession(snapshot.connectionId, target));
			if (!session) return;
			binding.sessionId = session;
			if (root() === target) setSessionId(session);
			await guard("saving conversation", () => remember(target, session));
			if (snapshot.capabilities?.list)
				await guard("listing conversations", () => refreshSessions(snapshot.connectionId, target));
		} finally {
			starting.delete(target);
			setConnecting(false);
		}
	}

	// Nothing is launched until the panel is on screen and a binary is named.
	// The store is deliberately not read here: a connection whose state changed
	// would re-run this effect, and a re-run mid-launch is how a second process
	// gets started.
	createEffect(() => {
		const target = root();
		if (!active() || !target || !settingsStore.isAcpConfigured()) return;
		const known = bindings.get(target);
		if (known) {
			setConnectionId(known.connectionId);
			setSessionId(known.sessionId);
			return;
		}
		setConnectionId(null);
		setSessionId(null);
		setListedSessions([]);
		void open(target);
	});

	const connection = (): AcpConnectionSnapshot | null => {
		const id = connectionId();
		return id ? acpStore.connection(id) : null;
	};

	const attachment = (): AcpAttachmentSnapshot | null => {
		const id = connectionId();
		const session = sessionId();
		return id && session ? acpStore.attachment(id, session) : null;
	};

	/** Only this session's questions. Another session on the same connection may
	 *  be blocked on its own, and answering it from here would answer blind. */
	const interactions = (): AcpPendingInteraction[] => {
		const id = connectionId();
		const session = sessionId();
		if (!id || !session) return [];
		return acpStore.interactions(id).filter((interaction) => interaction.sessionId === session);
	};

	const entries = (): AcpTranscriptEntry[] => {
		const session = sessionId();
		return session ? acpTranscript.entries(session) : [];
	};

	const phase = (): AcpChatPhase => {
		if (!settingsStore.isAcpConfigured()) return "unconfigured";
		if (!root()) return "no-repo";
		if (error() && !sessionId()) return "failed";
		if (connecting() || !sessionId()) return "starting";
		return "live";
	};

	/** A turn is running, so the composer sends nothing and offers to stop. */
	const busy = (): boolean => {
		const state = attachment()?.state;
		return state === "prompting" || state === "cancelling";
	};

	const held = (): boolean => {
		const state = attachment()?.state;
		return state === "paused" || state === "pause_pending";
	};

	function pair(): { id: AcpConnectionId; session: AcpSessionId } | null {
		const id = connectionId();
		const session = sessionId();
		return id && session ? { id, session } : null;
	}

	return {
		root,
		connectionId,
		sessionId,
		phase,
		error,
		clearError: () => setError(null),
		connection,
		attachment,
		interactions,
		entries,
		title: () => {
			const session = sessionId();
			return session ? acpTranscript.title(session) : null;
		},
		usage: () => {
			const session = sessionId();
			return session ? acpTranscript.usage(session) : null;
		},
		busy,
		held,

		capabilities: () => connection()?.capabilities ?? null,
		configOptions: (): AcpSessionConfigOption[] => attachment()?.configOptions ?? [],
		gap: () => {
			const id = connectionId();
			return id ? acpStore.gap(id) : null;
		},
		isStreaming: () => {
			const id = connectionId();
			return id ? acpStore.isStreaming(id) : false;
		},
		/** Durable sessions published by ego, newest first. */
		sessions: listedSessions,

		async send(text: string, images: Extract<AcpContentBlock, { type: "image" }>[] = []): Promise<void> {
			const current = pair();
			if (!current || (!text.trim() && images.length === 0)) return;
			await guard("sending the turn", () =>
				images.length
					? client.prompt(current.id, current.session, text, images)
					: client.prompt(current.id, current.session, text),
			);
		},

		async cancel(): Promise<void> {
			const current = pair();
			if (!current) return;
			await guard("cancelling the turn", () => client.cancel(current.id, current.session));
		},

		async pause(): Promise<void> {
			const current = pair();
			if (!current) return;
			await guard("pausing the turn", () => client.pause(current.id, current.session));
		},

		async resume(): Promise<void> {
			const current = pair();
			if (!current) return;
			await guard("resuming the turn", () => client.resumeTurn(current.id, current.session));
		},

		async compact(): Promise<void> {
			const current = pair();
			if (!current) return;
			await guard("compacting the conversation", () => client.compact(current.id, current.session));
		},

		async setOption(configId: string, value: AcpSessionConfigOptionValue): Promise<void> {
			const current = pair();
			if (!current) return;
			await guard("setting a session option", () =>
				client.setConfigOption(current.id, current.session, configId, value),
			);
		},

		async answerPermission(requestId: AcpHostRequestId, optionId: string): Promise<void> {
			const id = connectionId();
			if (!id) return;
			await guard("answering a permission request", () => client.answerPermission(id, requestId, optionId));
		},

		async cancelPermission(requestId: AcpHostRequestId): Promise<void> {
			const id = connectionId();
			if (!id) return;
			await guard("dismissing a permission request", () => client.cancelPermission(id, requestId));
		},

		async answerElicitation(requestId: AcpHostRequestId, action: AcpElicitationAction): Promise<void> {
			const id = connectionId();
			if (!id) return;
			await guard("answering a form", () => client.answerElicitation(id, requestId, action));
		},

		/** Start a second conversation on the same repository. */
		async startSession(): Promise<void> {
			const id = connectionId();
			const target = root();
			if (!id || !target) return;
			const session = await guard("opening a session", () => client.newSession(id, target));
			if (!session) return;
			bindings.set(target, { connectionId: id, sessionId: session });
			setSessionId(session);
			await guard("saving conversation", () => remember(target, session));
			if (connection()?.capabilities?.list) await guard("listing conversations", () => refreshSessions(id, target));
		},

		/** Show a listed session. Only a session not already attached needs replay. */
		async selectSession(session: AcpSessionId): Promise<void> {
			const id = connectionId();
			const target = root();
			if (!id || !target || session === sessionId()) return;
			if (!acpStore.attachment(id, session)) {
				const loaded = await guard("loading conversation", () => client.loadSession(id, session, target));
				if (loaded === null && error()) return;
			}
			bindings.set(target, { connectionId: id, sessionId: session });
			setSessionId(session);
			await guard("saving conversation", () => remember(target, session));
		},

		/**
		 * Replace the process and pick the conversation back up.
		 *
		 * The recovery for a gap, and for a connection that died: the journal no
		 * longer holds what the cursor asks for, so a fresh process replays the
		 * history through `session/load` instead. An agent that cannot load starts
		 * a new session — an empty panel is honest, a silently truncated one is not.
		 */
		async recover(): Promise<void> {
			const target = root();
			const id = connectionId();
			if (!target || !id) return;
			const previous = sessionId();
			setConnecting(true);
			try {
				const snapshot = await guard("reconnecting to ego", () => client.reconnect(id, target));
				if (!snapshot) return;
				setConnectionId(snapshot.connectionId);
				bindings.set(target, { connectionId: snapshot.connectionId, sessionId: previous });
				if (snapshot.capabilities?.list)
					await guard("listing conversations", () => refreshSessions(snapshot.connectionId, target));
				let resumeId = previous;
				if (!resumeId) {
					const config = await guard("reading saved conversation", () =>
						invoke<{ ai_chat_sessions?: Record<string, string> }>("load_config"),
					);
					if (!config) return;
					resumeId = config.ai_chat_sessions?.[target] ?? null;
				}
				if (resumeId && snapshot.capabilities?.load) {
					bindings.set(target, { connectionId: snapshot.connectionId, sessionId: resumeId });
					setSessionId(resumeId);
					await guard("replaying the conversation", () => client.loadSession(snapshot.connectionId, resumeId, target));
					return;
				}
				const session = await guard("opening a session", () => client.newSession(snapshot.connectionId, target));
				if (!session) return;
				bindings.set(target, { connectionId: snapshot.connectionId, sessionId: session });
				setSessionId(session);
				await guard("saving conversation", () => remember(target, session));
				if (snapshot.capabilities?.list)
					await guard("listing conversations", () => refreshSessions(snapshot.connectionId, target));
			} finally {
				setConnecting(false);
			}
		},

		/** Stop ego for this root. The transcript stays: the panel is not where a
		 *  conversation is deleted. */
		async stop(): Promise<void> {
			const id = connectionId();
			const target = root();
			if (!id) return;
			if (target) bindings.delete(target);
			setConnectionId(null);
			setSessionId(null);
			await guard("disconnecting", () => client.disconnect(id));
		},
	};
}

export type AcpChat = ReturnType<typeof createAcpChat>;
