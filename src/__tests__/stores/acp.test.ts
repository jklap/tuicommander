import { beforeEach, describe, expect, it, vi } from "vitest";

// Mock the IPC boundary only. Everything this file tests — how a journal frame
// moves the cursor, which frames are terminal, when a gap becomes visible — is
// the store's own logic, and mocking it would test the mock.
vi.mock("../../invoke", () => ({ invoke: vi.fn() }));

import { invoke } from "../../invoke";
import { acpStore } from "../../stores/acp";
import type {
	AcpClientEvent,
	AcpConnectionSnapshot,
	AcpPendingInteraction,
	AcpRequestPermissionRequest,
} from "../../types/acp";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

const CONNECTION = "01932d5e-0000-7000-8000-0000000000c1";
const SESSION = "01932d5e-0000-7000-8000-0000000000aa";

function snapshot(overrides: Partial<AcpConnectionSnapshot> = {}): AcpConnectionSnapshot {
	return {
		connectionId: CONNECTION,
		generation: 1,
		state: "ready",
		agentInfo: { name: "ego", version: "0.1.0" },
		capabilities: null,
		attachments: [],
		earliestSequence: 1,
		latestSequence: 1,
		settlement: null,
		...overrides,
	};
}

function attachment(overrides: Record<string, unknown> = {}) {
	return {
		sessionId: SESSION,
		state: "idle" as const,
		cwd: "/repo",
		additionalDirectories: [],
		configOptions: [],
		usage: null,
		activeTurn: null,
		pendingPermissionIds: [],
		pendingElicitationIds: [],
		...overrides,
	};
}

/** One event frame at `sequence`, as it arrives off the stream. */
function frame(sequence: number, event: AcpClientEvent, sessionId: string | null = null) {
	return {
		kind: "event" as const,
		connectionId: CONNECTION,
		generation: 1,
		sequence,
		sessionId,
		turnId: null,
		event,
	};
}

function permissionRequest(): AcpRequestPermissionRequest {
	return {
		sessionId: SESSION,
		toolCall: {},
		options: [
			{ optionId: "allow", name: "Allow", kind: "allow_once" },
			{ optionId: "reject", name: "Reject", kind: "reject_once" },
		],
	};
}

function permission(requestId: string): AcpPendingInteraction {
	return { kind: "permission", requestId, sessionId: SESSION, request: permissionRequest() };
}

beforeEach(() => {
	mockInvoke.mockReset();
	mockInvoke.mockResolvedValue(undefined);
	acpStore.reset();
});

describe("acpStore: what it holds", () => {
	it("holds a connection and the attachments that came with it", () => {
		acpStore.applySnapshot(snapshot({ attachments: [attachment()] }));

		expect(acpStore.connection(CONNECTION)?.state).toBe("ready");
		expect(acpStore.attachments(CONNECTION).map((a) => a.sessionId)).toEqual([SESSION]);
		expect(acpStore.attachment(CONNECTION, SESSION)?.state).toBe("idle");
	});

	// The snapshot is the whole picture for one connection, so a later one
	// replaces it rather than merging into it. Merging would keep an attachment
	// the agent has since dropped, and nothing would ever remove it.
	it("replaces a connection's attachments rather than accumulating them", () => {
		acpStore.applySnapshot(snapshot({ attachments: [attachment()] }));
		acpStore.applySnapshot(snapshot({ attachments: [] }));

		expect(acpStore.attachments(CONNECTION)).toEqual([]);
	});

	it("keeps connections apart", () => {
		const other = "01932d5e-0000-7000-8000-0000000000c2";
		acpStore.applySnapshot(snapshot());
		acpStore.applySnapshot(snapshot({ connectionId: other, attachments: [attachment()] }));

		expect(acpStore.attachments(CONNECTION)).toEqual([]);
		expect(acpStore.attachments(other)).toHaveLength(1);
	});
});

describe("acpStore: the journal cursor", () => {
	// The cursor is what a resume is computed from, so it has to move on every
	// frame the store accepted — not on the frames it found interesting.
	it("advances to the sequence of the last frame handled", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(frame(4, { kind: "turnStarted" }, SESSION));
		acpStore.applyFrame(frame(5, { kind: "turnSettled", stopReason: "end_turn", usage: null }, SESSION));

		expect(acpStore.cursor(CONNECTION)).toBe(5);
	});

	// `subscribe` delivers `>= from`, so resuming at the last sequence handled
	// re-delivers it. One past it is the only value that neither repeats a frame
	// nor skips one.
	it("resumes one past the last sequence handled", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(frame(7, { kind: "turnStarted" }, SESSION));

		expect(acpStore.resumeFrom(CONNECTION)).toBe(8);
	});

	// Nothing handled yet means everything the journal still holds, which is
	// what 0 asks for. Asking for 1 would skip the first event on a connection
	// whose journal has not yet rolled.
	it("resumes from the beginning when nothing has been handled", () => {
		acpStore.applySnapshot(snapshot());

		expect(acpStore.resumeFrom(CONNECTION)).toBe(0);
	});

	// The journal is ordered and the transport delivers in order, so an
	// out-of-order sequence is not a late frame to be slotted in — it is a bug
	// somewhere upstream, and letting it drag the cursor backwards would replay
	// everything after it on the next resume.
	it("never moves the cursor backwards", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(frame(9, { kind: "turnStarted" }, SESSION));
		acpStore.applyFrame(frame(3, { kind: "turnStarted" }, SESSION));

		expect(acpStore.cursor(CONNECTION)).toBe(9);
	});
});

describe("acpStore: state events", () => {
	it("moves the connection to the state a connectionState frame reports", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(frame(2, { kind: "connectionState", state: "failed" }));

		expect(acpStore.connection(CONNECTION)?.state).toBe("failed");
	});

	// The event names no session; the envelope does. A store that read the
	// session off the event would move every attachment at once.
	it("moves only the attachment the envelope names", () => {
		const other = "01932d5e-0000-7000-8000-0000000000ab";
		acpStore.applySnapshot(snapshot({ attachments: [attachment(), attachment({ sessionId: other })] }));
		acpStore.applyFrame(frame(2, { kind: "attachmentState", state: "prompting" }, SESSION));

		expect(acpStore.attachment(CONNECTION, SESSION)?.state).toBe("prompting");
		expect(acpStore.attachment(CONNECTION, other)?.state).toBe("idle");
	});
});

describe("acpStore: pending interactions", () => {
	it("holds a question until it is settled", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyInteractions(CONNECTION, [permission("req-1")]);

		expect(acpStore.interactions(CONNECTION)).toHaveLength(1);

		acpStore.applyFrame(
			frame(3, { kind: "permissionSettled", requestId: "req-1", outcome: { outcome: "cancelled" } }, SESSION),
		);

		expect(acpStore.interactions(CONNECTION)).toEqual([]);
	});

	// A question may be answered by another client, or by a phone. The settle
	// frame is the only thing that retracts it, so it must retract it whether or
	// not this window is the one that answered.
	it("drops a question settled by somebody else", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(
			frame(2, { kind: "permissionRequested", requestId: "req-2", request: permissionRequest() }, SESSION),
		);

		expect(acpStore.interactions(CONNECTION).map((i) => i.requestId)).toEqual(["req-2"]);

		acpStore.applyFrame(
			frame(
				3,
				{ kind: "permissionSettled", requestId: "req-2", outcome: { outcome: "selected", optionId: "allow" } },
				SESSION,
			),
		);

		expect(acpStore.interactions(CONNECTION)).toEqual([]);
	});

	it("does not add the same question twice", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyInteractions(CONNECTION, [permission("req-3")]);
		acpStore.applyFrame(
			frame(4, { kind: "permissionRequested", requestId: "req-3", request: permissionRequest() }, SESSION),
		);

		expect(acpStore.interactions(CONNECTION)).toHaveLength(1);
	});
});

describe("acpStore: a gap is a state, not a dropped frame", () => {
	// A gap says the journal no longer holds the sequence asked for. Skipping
	// ahead and carrying on would leave the panel showing a conversation with a
	// hole in it and no way to know, which is the whole reason criterion 6 asks
	// for it to be surfaced.
	it("records the gap and stops treating the stream as live", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(frame(2, { kind: "turnStarted" }, SESSION));
		acpStore.applyFrame({
			kind: "gap",
			code: "stream_gap",
			message: "sequence 3 is no longer held; the earliest is 40",
			connectionId: CONNECTION,
			sessionId: null,
			operation: null,
			retryable: false,
		});

		expect(acpStore.gap(CONNECTION)?.code).toBe("stream_gap");
		expect(acpStore.isStreaming(CONNECTION)).toBe(false);
	});

	// A gap is terminal for the subscription, and the producer sends nothing
	// after it. Advancing the cursor past a gap would make the next resume ask
	// for events that were never delivered.
	it("leaves the cursor where the last real frame left it", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame(frame(2, { kind: "turnStarted" }, SESSION));
		acpStore.applyFrame({
			kind: "gap",
			code: "stream_gap",
			message: "gone",
			connectionId: CONNECTION,
			sessionId: null,
			operation: null,
			retryable: false,
		});

		expect(acpStore.cursor(CONNECTION)).toBe(2);
	});

	it("clears the gap when the connection is subscribed again", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.applyFrame({
			kind: "gap",
			code: "stream_gap",
			message: "gone",
			connectionId: CONNECTION,
			sessionId: null,
			operation: null,
			retryable: false,
		});
		acpStore.markStreaming(CONNECTION);

		expect(acpStore.gap(CONNECTION)).toBeNull();
		expect(acpStore.isStreaming(CONNECTION)).toBe(true);
	});

	// Giving up on the stream is a third ending, and the one a person can undo.
	// Recording it as a gap would send them to recover a conversation that is
	// intact; leaving it as streaming would show a live panel reading nothing.
	it("stops the stream without recording a gap when the reader gives up", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.markStreaming(CONNECTION);
		acpStore.markStopped(CONNECTION);

		expect(acpStore.isStreaming(CONNECTION)).toBe(false);
		expect(acpStore.gap(CONNECTION)).toBeNull();
		expect(acpStore.connection(CONNECTION)?.state).toBe("ready");
	});

	// `end` is the other terminal frame and means the opposite of a gap: the
	// stream finished with nothing missing. Reporting it as a gap would send a
	// person to recover a conversation that is intact.
	it("ends the stream without recording a gap", () => {
		acpStore.applySnapshot(snapshot());
		acpStore.markStreaming(CONNECTION);
		acpStore.applyFrame({ kind: "end" });

		expect(acpStore.isStreaming(CONNECTION)).toBe(false);
		expect(acpStore.gap(CONNECTION)).toBeNull();
	});
});

describe("acpStore: frames for a connection it does not know", () => {
	// A frame naming a connection the store dropped is not an error to raise at
	// a person — a disconnect races the last frames in flight. It is an error to
	// resurrect the connection from, though: a half-built entry with no snapshot
	// behind it renders as a connection that does not exist.
	it("is ignored rather than creating a connection", () => {
		acpStore.applyFrame(frame(2, { kind: "turnStarted" }, SESSION));

		expect(acpStore.connection(CONNECTION)).toBeNull();
		expect(acpStore.cursor(CONNECTION)).toBe(0);
	});
});
