import { createStore, produce } from "solid-js/store";
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
	/** Next entry id. Monotonic across sessions; only distinctness matters. */
	nextId: number;
}

const [state, setState] = createStore<TranscriptState>({ sessions: {}, nextId: 1 });

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

function reduceUpdate(draft: TranscriptState, entries: AcpTranscriptEntry[], update: AcpSessionUpdate): void {
	const record = update as unknown as Record<string, unknown>;
	switch (update.sessionUpdate) {
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
			// `config_option_update`, `usage_update` and anything a later protocol
			// version adds are not conversation. They reach the panel through the
			// connection snapshot, which is refetched, so dropping them here loses
			// nothing.
			break;
	}
}

export const acpTranscript = {
	state,

	/** Forget everything. Tests only. */
	reset(): void {
		setState({ sessions: {}, nextId: 1 });
	},

	/**
	 * Drop one session's transcript.
	 *
	 * Called before a `session/load`, which replays the whole history: without
	 * it the replay lands under what is already there and every message appears
	 * twice.
	 */
	clear(sessionId: AcpSessionId): void {
		setState(
			produce((s: TranscriptState) => {
				delete s.sessions[sessionId];
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
					reduceUpdate(s, entries, event.update);
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

	/** What the user typed, shown before the agent has echoed it back. */
	noteUserMessage(sessionId: AcpSessionId, text: string): void {
		setState(
			produce((s: TranscriptState) => {
				const entries = (s.sessions[sessionId] ??= []);
				entries.push({ id: `e${s.nextId}`, kind: "user", text });
				s.nextId += 1;
			}),
		);
	},

	entries(sessionId: AcpSessionId): AcpTranscriptEntry[] {
		return state.sessions[sessionId] ?? [];
	},
};
