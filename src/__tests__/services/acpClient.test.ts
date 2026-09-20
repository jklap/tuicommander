import { beforeEach, describe, expect, it, vi } from "vitest";

// The IPC boundary and the frame transport are the two edges of this module.
// Both are replaced here because what is under test is neither of them: it is
// what the client does with a cursor, a gap and a dropped socket.
vi.mock("../../invoke", () => ({ invoke: vi.fn() }));

import { invoke } from "../../invoke";
import { createAcpClient } from "../../services/acpClient";
import type { AcpStreamHandle, AcpStreamOptions } from "../../services/acpStream";
import { acpStore } from "../../stores/acp";
import type { AcpConnectionSnapshot, AcpStreamFrame } from "../../types/acp";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

const CONNECTION = "01932d5e-0000-7000-8000-0000000000c1";
const SESSION = "01932d5e-0000-7000-8000-0000000000aa";
const ROOT = "/repo";

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

/** Every stream the client opened, in order, with the frames it was given. */
class FakeStreams {
	opened: AcpStreamOptions[] = [];
	closed = 0;
	private failNext = 0;

	readonly open = async (options: AcpStreamOptions): Promise<AcpStreamHandle> => {
		this.opened.push(options);
		if (this.failNext > 0) {
			this.failNext -= 1;
			throw new Error("the stream refused to open");
		}
		return { close: () => void (this.closed += 1) };
	};

	failOpens(times: number): void {
		this.failNext = times;
	}

	get last(): AcpStreamOptions {
		const last = this.opened.at(-1);
		if (!last) throw new Error("no stream was opened");
		return last;
	}

	deliver(frame: AcpStreamFrame): void {
		this.last.onFrame(frame);
	}

	drop(): void {
		this.last.onDropped?.();
	}
}

function frame(sequence: number): AcpStreamFrame {
	return {
		kind: "event",
		connectionId: CONNECTION,
		generation: 1,
		sequence,
		sessionId: SESSION,
		turnId: null,
		event: { kind: "turnStarted" },
	};
}

const GAP: AcpStreamFrame = {
	kind: "gap",
	code: "stream_gap",
	message: "sequence 3 is no longer held",
	connectionId: CONNECTION,
	sessionId: null,
	operation: null,
	retryable: false,
};

/** Answer each command with what its Rust counterpart returns. */
function answering(overrides: Record<string, unknown> = {}) {
	return (command: string) => {
		if (command in overrides) return Promise.resolve(overrides[command]);
		switch (command) {
			case "acp_connect":
			case "acp_reconnect":
			case "acp_connection_snapshot":
				return Promise.resolve(snapshot());
			case "acp_pending_interactions":
				return Promise.resolve([]);
			case "acp_session_new":
			case "acp_session_load":
				return Promise.resolve({ sessionId: SESSION, state: "idle", cwd: ROOT });
			default:
				return Promise.resolve(undefined);
		}
	};
}

let streams: FakeStreams;
let client: ReturnType<typeof createAcpClient>;

beforeEach(() => {
	mockInvoke.mockReset();
	mockInvoke.mockImplementation(answering());
	acpStore.reset();
	streams = new FakeStreams();
	client = createAcpClient(streams.open);
});

describe("acpClient: opening a connection", () => {
	it("holds the snapshot and starts reading the journal from its beginning", async () => {
		await client.connect(ROOT);

		expect(mockInvoke).toHaveBeenCalledWith("acp_connect", { root: ROOT });
		expect(acpStore.connection(CONNECTION)?.state).toBe("ready");
		expect(streams.opened).toHaveLength(1);
		expect(streams.last.afterSequence).toBe(0);
		expect(acpStore.isStreaming(CONNECTION)).toBe(true);
	});

	// A question raised before this window subscribed is still waiting for an
	// answer, and no frame will announce it again. Without this the panel shows
	// an idle session that is in fact blocked on a permission nobody can see.
	it("takes the questions that were already pending", async () => {
		mockInvoke.mockImplementation(
			answering({
				acp_pending_interactions: [
					{
						kind: "permission",
						requestId: "req-1",
						sessionId: SESSION,
						request: { sessionId: SESSION, toolCall: {}, options: [] },
					},
				],
			}),
		);

		await client.connect(ROOT);

		expect(acpStore.interactions(CONNECTION).map((i) => i.requestId)).toEqual(["req-1"]);
	});
});

describe("acpClient: a stream that stops", () => {
	// The socket dropping says nothing about the journal — the events are still
	// there. Resuming at the cursor is what makes a dropped connection invisible
	// to the person reading the panel.
	it("resumes one past the last sequence it handled", async () => {
		await client.connect(ROOT);
		streams.deliver(frame(4));
		streams.drop();
		await vi.waitFor(() => expect(streams.opened).toHaveLength(2));

		expect(streams.last.afterSequence).toBe(5);
	});

	// A gap is terminal and not recoverable by reading again: the journal no
	// longer holds what the cursor asks for, so resuming would ask for it
	// forever. The store makes it visible instead, and a person decides.
	it("does not resume after a gap", async () => {
		await client.connect(ROOT);
		streams.deliver(frame(2));
		streams.deliver(GAP);
		streams.drop();
		await vi.waitFor(() => expect(acpStore.gap(CONNECTION)?.code).toBe("stream_gap"));

		expect(streams.opened).toHaveLength(1);
		expect(acpStore.isStreaming(CONNECTION)).toBe(false);
	});

	// `end` means the journal finished with nothing missing — a settled
	// connection. Resuming would reopen a stream that immediately ends again.
	it("does not resume after the stream ended", async () => {
		await client.connect(ROOT);
		streams.deliver({ kind: "end" });
		streams.drop();

		expect(streams.opened).toHaveLength(1);
	});

	// A backend that is gone answers every open the same way. Retrying without
	// a bound turns one dead connection into a spin; stopping leaves a state the
	// panel can render and a person can act on.
	it("gives up after a bounded number of failed resumes", async () => {
		await client.connect(ROOT);
		streams.failOpens(10);
		streams.drop();
		await vi.waitFor(() => expect(acpStore.isStreaming(CONNECTION)).toBe(false));

		expect(streams.opened.length).toBeLessThanOrEqual(4);
	});

	// A frame is proof the stream works, so the budget it spent getting there is
	// not owed by the next drop. Without this, a connection that is merely
	// long-lived eventually exhausts a budget that was never about it.
	it("gives the budget back once a frame arrives", async () => {
		await client.connect(ROOT);
		streams.failOpens(2);
		streams.drop();
		await vi.waitFor(() => expect(streams.opened.length).toBeGreaterThan(2));
		streams.deliver(frame(9));
		streams.failOpens(2);
		streams.drop();
		await vi.waitFor(() => expect(streams.opened.length).toBeGreaterThan(5));

		expect(acpStore.isStreaming(CONNECTION)).toBe(true);
	});
});

describe("acpClient: talking to a session", () => {
	it("sends a prompt as one text content block", async () => {
		await client.connect(ROOT);
		await client.prompt(CONNECTION, SESSION, "hello");

		expect(mockInvoke).toHaveBeenCalledWith("acp_session_prompt", {
			connectionId: CONNECTION,
			sessionId: SESSION,
			prompt: [{ type: "text", text: "hello" }],
		});
	});

	// The ids are the agent's. Answering with an Allow/Deny of this client's own
	// invention would answer a question nobody asked.
	it("answers a permission with one of the option ids the agent published", async () => {
		await client.connect(ROOT);
		await client.answerPermission(CONNECTION, "req-1", "allow_once_id");

		expect(mockInvoke).toHaveBeenCalledWith("acp_respond_permission", {
			connectionId: CONNECTION,
			requestId: "req-1",
			outcome: { outcome: "selected", optionId: "allow_once_id" },
		});
	});

	it("cancels a permission without naming an option", async () => {
		await client.connect(ROOT);
		await client.cancelPermission(CONNECTION, "req-1");

		expect(mockInvoke).toHaveBeenCalledWith("acp_respond_permission", {
			connectionId: CONNECTION,
			requestId: "req-1",
			outcome: { outcome: "cancelled" },
		});
	});

	// The session is opened against the root the connection runs on, and the
	// server decides what it may reach beyond that: a caller naming an MCP
	// server is refused by the backend, so the client never sends one.
	it("opens a session on the connection's own root and names nothing else", async () => {
		await client.connect(ROOT);
		await client.newSession(CONNECTION, ROOT);

		expect(mockInvoke).toHaveBeenCalledWith("acp_session_new", {
			connectionId: CONNECTION,
			authority: { cwd: ROOT, additionalDirectories: [] },
		});
	});

	// Pause, resume and compact are idempotency-keyed by a request id the client
	// mints. A backend that receives the same one twice answers once.
	it("mints a uuid request id for a hold", async () => {
		await client.connect(ROOT);
		await client.pause(CONNECTION, SESSION);

		const call = mockInvoke.mock.calls.find(([command]) => command === "acp_turn_pause");
		expect(call?.[1].requestId).toMatch(/^[0-9a-f-]{36}$/);
	});
});

describe("acpClient: letting go", () => {
	it("closes the stream and forgets the connection", async () => {
		await client.connect(ROOT);
		await client.disconnect(CONNECTION);

		expect(mockInvoke).toHaveBeenCalledWith("acp_disconnect", { connectionId: CONNECTION });
		expect(streams.closed).toBe(1);
		expect(acpStore.connection(CONNECTION)).toBeNull();
	});

	// A connection that is gone must not be resumed by the drop its own
	// teardown causes, which arrives after the close.
	it("does not resume a stream it closed itself", async () => {
		await client.connect(ROOT);
		await client.disconnect(CONNECTION);
		streams.drop();

		expect(streams.opened).toHaveLength(1);
	});
});
