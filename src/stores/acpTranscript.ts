import { createStore, produce, unwrap } from "solid-js/store";
import type {
	AcpPlanEntry,
	AcpSessionId,
	AcpSessionUpdate,
	AcpStreamFrame,
	AcpToolCall,
	AcpToolCallContent,
	AcpToolCallLocation,
} from "../types/acp";

/**
 * The conversation, as a person reads it.
 *
 * Kept apart from `acpStore` on purpose. That store holds protocol state — what
 * the connection is, where the cursor sits, which questions are open — and is
 * pinned field-for-field to the wire. This one holds a projection built for
 * rendering: chunks joined into a message, a tool call and its later updates
 * folded into one card, a plan replaced rather than appended. The two change at
 * different rates and for different reasons, and folding them together would
 * make every streamed character touch the object the connection list reads.
 *
 * Keyed by session rather than by connection: a session outlives the connection
 * that opened it — ego owns it, and reconnecting attaches to the same id — so a
 * transcript filed under the connection would be thrown away by a reconnect
 * that changed nothing a person can see.
 */

export type AcpTranscriptEntry =
	| { id: string; kind: "user"; text: string }
	| { id: string; kind: "agent"; text: string }
	| { id: string; kind: "thought"; text: string }
	| { id: string; kind: "tool"; call: AcpToolCall }
	| { id: string; kind: "plan"; entries: AcpPlanEntry[] }
	/** A turn that ended as something other than a finished answer. */
	| { id: string; kind: "settled"; stopReason: string };

interface TranscriptState {
	sessions: Record<AcpSessionId, AcpTranscriptEntry[]>;
	titles: Record<AcpSessionId, string>;
	usage: Record<AcpSessionId, { used: number; size: number; cost?: { amount: number; currency: string } }>;
	/** Next entry id. Monotonic across sessions; only distinctness matters. */
	nextId: number;
}

const [state, setState] = createStore<TranscriptState>({ sessions: {}, titles: {}, usage: {}, nextId: 1 });

/**
 * The text inside a content block, or "" for a block that carries none.
 *
 * An image or an embedded resource is not rendered as a message today, and
 * rendering its JSON instead would put a wall of base64 in the conversation.
 */
function textOf(content: unknown): string {
	if (content && typeof content === "object" && (content as { type?: string }).type === "text") {
		return String((content as { text?: unknown }).text ?? "");
	}
	return "";
}

/**
 * Append text to the last entry when it is the same kind, or start a new one.
 *
 * Chunks are how a turn streams: one word per frame is normal. Kept as separate
 * entries they render as one word per bubble, and joining them at render time
 * would mean re-deriving the grouping on every frame.
 */
function appendChunk(
	draft: TranscriptState,
	entries: AcpTranscriptEntry[],
	kind: "user" | "agent" | "thought",
	text: string,
): void {
	if (!text) return;
	const last = entries.at(-1);
	if (last?.kind === kind) {
		last.text += text;
		return;
	}
	entries.push({ id: `e${draft.nextId}`, kind, text });
	draft.nextId += 1;
}

/** Fold a tool call, or an update to one, into the single card that shows it. */
function foldToolCall(draft: TranscriptState, entries: AcpTranscriptEntry[], update: Record<string, unknown>): void {
	const toolCallId = String(update.toolCallId ?? "");
	if (!toolCallId) return;
	const existing = entries.find((entry) => entry.kind === "tool" && entry.call.toolCallId === toolCallId);
	const fields: Partial<AcpToolCall> = {
		title: update.title as string | undefined,
		kind: update.kind as AcpToolCall["kind"],
		status: update.status as AcpToolCall["status"],
		content: update.content as AcpToolCallContent[] | undefined,
		locations: update.locations as AcpToolCallLocation[] | undefined,
		rawInput: update.rawInput,
		rawOutput: update.rawOutput,
	};
	if (existing?.kind === "tool") {
		// An update names only what changed. Copying the undefined ones over
		// would erase a title and a location list the first frame carried.
		for (const [key, value] of Object.entries(fields)) {
			if (value !== undefined) (existing.call as unknown as Record<string, unknown>)[key] = value;
		}
		return;
	}
	entries.push({
		id: `e${draft.nextId}`,
		kind: "tool",
		// A title is required by the protocol on the first frame, but an update
		// for a call this client never saw — a session loaded mid-flight — has
		// none, and an empty card is better than a dropped one.
		call: { toolCallId, title: (fields.title as string) ?? toolCallId, ...stripUndefined(fields) },
	});
	draft.nextId += 1;
}

function stripUndefined(fields: Partial<AcpToolCall>): Partial<AcpToolCall> {
	return Object.fromEntries(Object.entries(fields).filter(([, value]) => value !== undefined));
}

function reduceUpdate(
	draft: TranscriptState,
	sessionId: AcpSessionId,
	entries: AcpTranscriptEntry[],
	update: AcpSessionUpdate,
): void {
	const record = update as unknown as Record<string, unknown>;
	switch (update.sessionUpdate) {
		case "session_info_update":
			if (typeof record.title === "string" && record.title.trim()) draft.titles[sessionId] = record.title;
			break;
		case "usage_update": {
			if (
				typeof record.used !== "number" ||
				!Number.isFinite(record.used) ||
				record.used < 0 ||
				typeof record.size !== "number" ||
				!Number.isFinite(record.size) ||
				record.size <= 0
			)
				break;
			const cost = record.cost;
			draft.usage[sessionId] = {
				used: record.used,
				size: record.size,
				...(cost &&
				typeof cost === "object" &&
				typeof (cost as { amount?: unknown }).amount === "number" &&
				Number.isFinite((cost as { amount: number }).amount) &&
				typeof (cost as { currency?: unknown }).currency === "string"
					? { cost: cost as { amount: number; currency: string } }
					: {}),
			};
			break;
		}
		case "user_message_chunk":
			appendChunk(draft, entries, "user", textOf(record.content));
			break;
		case "agent_message_chunk":
			appendChunk(draft, entries, "agent", textOf(record.content));
			break;
		case "agent_thought_chunk":
			appendChunk(draft, entries, "thought", textOf(record.content));
			break;
		case "tool_call":
		case "tool_call_update":
			foldToolCall(draft, entries, record);
			break;
		case "plan": {
			// The agent sends the whole plan every time and the client replaces
			// its copy — an appended plan would show every intermediate list.
			const planEntries = (record.entries ?? []) as AcpPlanEntry[];
			const plan = entries.find((entry) => entry.kind === "plan");
			if (plan?.kind === "plan") {
				plan.entries = planEntries;
				break;
			}
			entries.push({ id: `e${draft.nextId}`, kind: "plan", entries: planEntries });
			draft.nextId += 1;
			break;
		}
		default:
			// `config_option_update` and anything a later protocol
			// version adds are not rendered by the transcript projection.
			break;
	}
}

export const acpTranscript = {
	state,

	/** Forget everything. Tests only. */
	reset(): void {
		setState({ sessions: {}, titles: {}, usage: {}, nextId: 1 });
	},

	/**
	 * Drop one session's transcript and hand back what was dropped.
	 *
	 * Called before a `session/load`, which replays the whole history: without
	 * it the replay lands under what is already there and every message appears
	 * twice. The return value is what makes that safe to do *before* the load —
	 * a load that is refused leaves the panel live on a session whose
	 * conversation has been erased, and only the caller knows the request failed.
	 */
	clear(sessionId: AcpSessionId): AcpTranscriptEntry[] {
		// Unwrapped and copied: what comes back has to outlive the store node it
		// came from, and handing back a live proxy to an array this call is about
		// to delete is how a restore puts back an empty transcript.
		const removed = [...(unwrap(state.sessions[sessionId]) ?? [])];
		setState(
			produce((s: TranscriptState) => {
				delete s.sessions[sessionId];
			}),
		);
		return removed;
	},

	/**
	 * Put back a transcript `clear` removed, for a load that never happened.
	 *
	 * Replaces rather than merges, and does nothing for an empty list: the only
	 * caller is undoing its own `clear`, so anything now under that session id
	 * arrived after the failure and is fresher than what is being restored.
	 */
	restore(sessionId: AcpSessionId, entries: AcpTranscriptEntry[]): void {
		if (entries.length === 0) return;
		setState(
			produce((s: TranscriptState) => {
				s.sessions[sessionId] = entries;
			}),
		);
	},

	/**
	 * Read one frame for the conversation it belongs to.
	 *
	 * A frame with no session — a connection-level state change, a gap — is not
	 * part of any conversation and is dropped here; `acpStore` is where those
	 * are held.
	 */
	applyFrame(frame: AcpStreamFrame): void {
		if (frame.kind !== "event" || !frame.sessionId) return;
		const sessionId = frame.sessionId;
		const event = frame.event;
		setState(
			produce((s: TranscriptState) => {
				const entries = (s.sessions[sessionId] ??= []);
				if (event.kind === "sessionUpdate") {
					reduceUpdate(s, sessionId, entries, event.update);
					return;
				}
				if (event.kind === "turnSettled" && event.stopReason !== "end_turn") {
					// A turn that ended because it was cancelled, refused or ran
					// out of room ended without answering, and a transcript that
					// just stops there reads as the agent falling silent.
					entries.push({ id: `e${s.nextId}`, kind: "settled", stopReason: event.stopReason });
					s.nextId += 1;
				}
			}),
		);
	},

	/**
	 * What the user typed, shown before the agent has echoed it back.
	 *
	 * Returns the entry's id so the caller can take it back. The message is put
	 * on screen before the prompt is sent — that is the point of it — so a prompt
	 * the backend refuses would otherwise leave a turn the agent never received
	 * sitting in the conversation, indistinguishable from one it ignored.
	 */
	noteUserMessage(sessionId: AcpSessionId, text: string): string {
		const id = `e${state.nextId}`;
		setState(
			produce((s: TranscriptState) => {
				const entries = (s.sessions[sessionId] ??= []);
				entries.push({ id, kind: "user", text });
				s.nextId += 1;
			}),
		);
		return id;
	},

	/**
	 * Take back an entry that turned out not to have happened.
	 *
	 * By id rather than by position: frames keep arriving while a prompt is in
	 * flight, so "the last entry" is not reliably the one being withdrawn.
	 */
	dropEntry(sessionId: AcpSessionId, entryId: string): void {
		setState(
			produce((s: TranscriptState) => {
				const entries = s.sessions[sessionId];
				if (!entries) return;
				const index = entries.findIndex((entry) => entry.id === entryId);
				if (index >= 0) entries.splice(index, 1);
			}),
		);
	},

	entries(sessionId: AcpSessionId): AcpTranscriptEntry[] {
		return state.sessions[sessionId] ?? [];
	},

	title(sessionId: AcpSessionId): string | null {
		return state.titles[sessionId] ?? null;
	},

	usage(sessionId: AcpSessionId): { used: number; size: number; cost?: { amount: number; currency: string } } | null {
		return state.usage[sessionId] ?? null;
	},
};
