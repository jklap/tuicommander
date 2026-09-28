import { beforeEach, describe, expect, it } from "vitest";

import { acpTranscript } from "../../stores/acpTranscript";
import type { AcpClientEvent, AcpStreamFrame } from "../../types/acp";

const SESSION = "01932d5e-0000-7000-8000-0000000000aa";
const OTHER = "01932d5e-0000-7000-8000-0000000000ab";

let sequence = 0;

function frame(event: AcpClientEvent, sessionId: string | null = SESSION): AcpStreamFrame {
	sequence += 1;
	return {
		kind: "event",
		connectionId: "01932d5e-0000-7000-8000-0000000000c1",
		generation: 1,
		sequence,
		sessionId,
		turnId: null,
		event,
	};
}

/** One `session/update`, in the shape the wire carries it. */
function update(body: Record<string, unknown>, sessionId: string | null = SESSION): AcpStreamFrame {
	return frame({ kind: "sessionUpdate", update: body } as unknown as AcpClientEvent, sessionId);
}

function text(body: string) {
	return { type: "text", text: body };
}

beforeEach(() => {
	sequence = 0;
	acpTranscript.reset();
});

describe("acpTranscript: messages", () => {
	// A turn streams one chunk at a time. Kept apart they render as one bubble
	// per word; joined at render time the grouping is re-derived every frame.
	it("joins consecutive chunks of the same kind into one message", () => {
		acpTranscript.applyFrame(update({ sessionUpdate: "agent_message_chunk", content: text("one ") }));
		acpTranscript.applyFrame(update({ sessionUpdate: "agent_message_chunk", content: text("two") }));

		expect(acpTranscript.entries(SESSION)).toEqual([{ id: "e1", kind: "agent", text: "one two" }]);
	});

	// Thinking is not the answer. Merging the two would put reasoning into the
	// message a person quotes back.
	it("keeps thoughts apart from the answer", () => {
		acpTranscript.applyFrame(update({ sessionUpdate: "agent_thought_chunk", content: text("hmm") }));
		acpTranscript.applyFrame(update({ sessionUpdate: "agent_message_chunk", content: text("hello") }));
		acpTranscript.applyFrame(update({ sessionUpdate: "agent_thought_chunk", content: text("more") }));

		expect(acpTranscript.entries(SESSION).map((e) => e.kind)).toEqual(["thought", "agent", "thought"]);
	});

	// An image or an embedded resource carries no text. Rendering its JSON
	// instead would put a wall of base64 in the conversation.
	it("ignores a content block that carries no text", () => {
		acpTranscript.applyFrame(
			update({ sessionUpdate: "agent_message_chunk", content: { type: "image", mimeType: "image/png", data: "AAAA" } }),
		);

		expect(acpTranscript.entries(SESSION)).toEqual([]);
	});

	it("files each session's conversation under its own id", () => {
		acpTranscript.applyFrame(update({ sessionUpdate: "agent_message_chunk", content: text("here") }));
		acpTranscript.applyFrame(update({ sessionUpdate: "agent_message_chunk", content: text("there") }, OTHER));

		expect(acpTranscript.entries(SESSION)).toHaveLength(1);
		expect(acpTranscript.entries(OTHER)).toHaveLength(1);
	});

	// A connection-level frame belongs to no conversation. Filing it under one
	// would need a session id this frame does not have.
	it("drops a frame that names no session", () => {
		acpTranscript.applyFrame(frame({ kind: "connectionState", state: "failed" }, null));

		expect(acpTranscript.entries(SESSION)).toEqual([]);
	});
});

describe("acpTranscript: tool calls", () => {
	// One call is one card. A second card per update would show a file being
	// read three times because it was reported pending, running and done.
	it("folds updates into the card the call opened", () => {
		acpTranscript.applyFrame(
			update({
				sessionUpdate: "tool_call",
				toolCallId: "t1",
				title: "Read AGENTS.md",
				kind: "read",
				status: "pending",
			}),
		);
		acpTranscript.applyFrame(update({ sessionUpdate: "tool_call_update", toolCallId: "t1", status: "completed" }));

		const entries = acpTranscript.entries(SESSION);
		expect(entries).toHaveLength(1);
		expect(entries[0]).toMatchObject({ kind: "tool", call: { title: "Read AGENTS.md", status: "completed" } });
	});

	// An update names only what changed, so copying its absent fields over
	// would erase the title and the locations the first frame carried.
	it("keeps what an update did not mention", () => {
		acpTranscript.applyFrame(
			update({
				sessionUpdate: "tool_call",
				toolCallId: "t1",
				title: "Edit src/main.rs",
				locations: [{ path: "src/main.rs", line: 4 }],
			}),
		);
		acpTranscript.applyFrame(update({ sessionUpdate: "tool_call_update", toolCallId: "t1", status: "failed" }));

		expect(acpTranscript.entries(SESSION)[0]).toMatchObject({
			call: { title: "Edit src/main.rs", locations: [{ path: "src/main.rs", line: 4 }], status: "failed" },
		});
	});

	// A session attached mid-flight replays updates for calls that started
	// before this client was listening. Dropping them would leave the work
	// invisible; an untitled card at least says something ran.
	it("shows an update for a call it never saw open", () => {
		acpTranscript.applyFrame(update({ sessionUpdate: "tool_call_update", toolCallId: "t9", status: "in_progress" }));

		expect(acpTranscript.entries(SESSION)[0]).toMatchObject({ kind: "tool", call: { toolCallId: "t9", title: "t9" } });
	});
});

describe("acpTranscript: the plan", () => {
	// The agent sends the whole plan every time and the client replaces its
	// copy. Appending would show every intermediate version of the list.
	it("replaces the plan rather than appending another one", () => {
		acpTranscript.applyFrame(
			update({ sessionUpdate: "plan", entries: [{ content: "one", priority: "high", status: "pending" }] }),
		);
		acpTranscript.applyFrame(
			update({
				sessionUpdate: "plan",
				entries: [
					{ content: "one", priority: "high", status: "completed" },
					{ content: "two", priority: "low", status: "pending" },
				],
			}),
		);

		const plans = acpTranscript.entries(SESSION).filter((e) => e.kind === "plan");
		expect(plans).toHaveLength(1);
		expect(plans[0]).toMatchObject({ entries: [{ status: "completed" }, { content: "two" }] });
	});
});

describe("acpTranscript: how a turn ended", () => {
	it("does not treat a previous answer as a reply to the next empty turn", () => {
		acpTranscript.applyFrame(frame({ kind: "promptSent", text: "first" }));
		acpTranscript.applyFrame(update({ sessionUpdate: "agent_message_chunk", content: text("done") }));
		acpTranscript.applyFrame(frame({ kind: "turnSettled", stopReason: "end_turn", usage: null }));
		acpTranscript.applyFrame(frame({ kind: "promptSent", text: "second" }));
		acpTranscript.applyFrame(frame({ kind: "turnSettled", stopReason: "end_turn", usage: null }));
		expect(acpTranscript.entries(SESSION).at(-1)).toMatchObject({ kind: "settled", stopReason: "empty" });
	});

	// A finished answer needs no marker — the answer is the marker.
	it("says nothing when a turn ended normally", () => {
		acpTranscript.applyFrame(update({ sessionUpdate: "agent_message_chunk", content: text("done") }));
		acpTranscript.applyFrame(frame({ kind: "turnSettled", stopReason: "end_turn", usage: null }));

		expect(acpTranscript.entries(SESSION).map((e) => e.kind)).toEqual(["agent"]);
	});

	// Cancelled, refused or out of room all end the turn without answering, and
	// a transcript that just stops there reads as the agent falling silent.
	it("records a turn that ended without answering", () => {
		acpTranscript.applyFrame(frame({ kind: "turnSettled", stopReason: "cancelled", usage: null }));

		expect(acpTranscript.entries(SESSION)).toEqual([{ id: "e1", kind: "settled", stopReason: "cancelled" }]);
	});
});

describe("acpTranscript: starting over", () => {
	// `session/load` replays the whole history. Without clearing, the replay
	// lands under what is already there and every message appears twice.
	it("drops one session's history and leaves the others", () => {
		acpTranscript.applyFrame(update({ sessionUpdate: "agent_message_chunk", content: text("old") }));
		acpTranscript.applyFrame(update({ sessionUpdate: "agent_message_chunk", content: text("kept") }, OTHER));
		acpTranscript.clear(SESSION);

		expect(acpTranscript.entries(SESSION)).toEqual([]);
		expect(acpTranscript.entries(OTHER)).toHaveLength(1);
	});

	// What a person typed shows immediately, before any frame comes back.
	it("holds the message the user sent", () => {
		acpTranscript.noteUserMessage(SESSION, "hello");

		expect(acpTranscript.entries(SESSION)).toEqual([{ id: "e1", kind: "user", text: "hello" }]);
	});
});
