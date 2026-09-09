import { describe, expect, it } from "vitest";
import type { ConversationMessage } from "../../stores/conversationStore";
import {
	type AiChatProjectionLocal,
	type AiChatSnapshot,
	type AiChatSnapshotSource,
	buildAiChatSnapshot,
	projectAiChat,
} from "../../utils/aiChatSnapshot";

/**
 * The detached AI Chat window is NOT a viewer: it owns a conversation store of
 * its own and sends against the terminal it was handed. The projection therefore
 * has two writers for one screen, and the whole point of these tests is which
 * one wins.
 *
 * The rule is an overlay, never a replacement: the projection carries only the
 * live stream, never the message history, so a snapshot that is stale can do no
 * damage — an idle snapshot writes nothing at all. What decides ownership is the
 * `mirroring` flag, because "the local store is streaming" is true both when the
 * user typed here and when we put a mirrored stream there ourselves.
 */

const snapshot = (over: Partial<AiChatSnapshot> = {}): AiChatSnapshot => ({
	chatId: "conv-1",
	isStreaming: false,
	streamingText: "",
	isThinking: false,
	error: null,
	agentState: "idle",
	textChunks: "",
	toolCalls: [],
	lastAssistantText: null,
	lastAssistantAt: null,
	...over,
});

/** A finished reply as the main window would report it. */
const reply = (text: string, at: number): Partial<AiChatSnapshot> => ({
	lastAssistantText: text,
	lastAssistantAt: at,
});

const local = (over: Partial<AiChatProjectionLocal> = {}): AiChatProjectionLocal => ({
	chatId: "conv-1",
	isStreaming: false,
	agentState: "idle",
	mirroring: false,
	mirroredFromAt: null,
	...over,
});

describe("projectAiChat", () => {
	// The defect the projection exists for: `PanelOrchestrator` unmounts the
	// docked panel while the chat is detached, but a watcher rule, an automation
	// goal and the terminal context menu all still start conversations on the
	// MAIN window's store. Those replies rendered nowhere at all.
	it("mirrors a stream the main window is running", () => {
		const result = projectAiChat(snapshot({ isStreaming: true, streamingText: "par" }), local());

		expect(result.overlay).toEqual({
			isStreaming: true,
			streamingText: "par",
			isThinking: false,
			error: null,
			agentState: "idle",
			textChunks: "",
			toolCalls: [],
		});
		expect(result.mirroring).toBe(true);
		expect(result.finalize).toBeNull();
	});

	// Mirroring sets `isStreaming` on the local store, so "the local store is
	// streaming" cannot by itself mean "the user owns this window". Without the
	// `mirroring` flag the second tick of every mirrored stream would be read as
	// a local stream and dropped, and the text would freeze after one tick.
	it("keeps mirroring across ticks even though the local store now reads as streaming", () => {
		const result = projectAiChat(
			snapshot({ isStreaming: true, streamingText: "partial reply" }),
			local({ isStreaming: true, mirroring: true }),
		);

		expect(result.overlay?.streamingText).toBe("partial reply");
		expect(result.mirroring).toBe(true);
	});

	// The dual-writer guard. A reply the user asked for in THIS window is the one
	// thing the projection must never paint over — same class of bug as re-reading
	// disk on top of a live stream.
	it("refuses to overwrite a stream the detached window started itself", () => {
		const result = projectAiChat(
			snapshot({ isStreaming: true, streamingText: "from the main window" }),
			local({ isStreaming: true, mirroring: false }),
		);

		expect(result.overlay).toBeNull();
		expect(result.mirroring).toBe(false);
	});

	it("refuses to overwrite an autonomous run the detached window started itself", () => {
		const result = projectAiChat(
			snapshot({ isStreaming: true, streamingText: "from the main window" }),
			local({ agentState: "running", mirroring: false }),
		);

		expect(result.overlay).toBeNull();
	});

	// A paused agent is still the local window's, even though nothing is arriving.
	it("treats a paused local agent as locally owned", () => {
		const result = projectAiChat(
			snapshot({ isStreaming: true, streamingText: "x" }),
			local({ agentState: "paused", mirroring: false }),
		);

		expect(result.overlay).toBeNull();
	});

	// The staleness guard. This window stays pinned to the terminal it was
	// detached with, so the main window can perfectly well be streaming into a
	// different terminal's conversation. That reply belongs on another screen.
	it("ignores a stream that belongs to another conversation", () => {
		const result = projectAiChat(
			snapshot({ chatId: "conv-9", isStreaming: true, streamingText: "someone else's" }),
			local({ chatId: "conv-1" }),
		);

		expect(result.overlay).toBeNull();
		expect(result.mirroring).toBe(false);
	});

	// Why staleness is survivable at all: the projection never carries the message
	// history, so the worst a snapshot from an hour ago can say is "nothing is
	// streaming" — and that writes nothing. A projection that replaced state would
	// instead wipe whatever the user typed here.
	it("writes nothing at all for an idle snapshot", () => {
		const result = projectAiChat(snapshot(), local());

		expect(result.overlay).toBeNull();
		expect(result.finalize).toBeNull();
	});

	it("writes nothing for an idle snapshot even when the local window holds an error", () => {
		const result = projectAiChat(snapshot(), local({ mirroring: false }));

		expect(result.overlay).toBeNull();
	});

	// The main window clears `streamingText` the instant the reply completes, and
	// the projection ticks at 250ms — so the last chunk this window mirrored is
	// very likely a tick short of the whole answer. Finalize from the snapshot's
	// copy of the finished message, not from what was mirrored.
	it("keeps the finished reply on screen when the mirrored stream ends", () => {
		const result = projectAiChat(
			snapshot({ isStreaming: false, streamingText: "", ...reply("the whole reply", 1000) }),
			local({ mirroring: true }),
		);

		expect(result.finalize).toBe("the whole reply");
		expect(result.overlay).toEqual({
			isStreaming: false,
			streamingText: "",
			isThinking: false,
			error: null,
			agentState: "idle",
			textChunks: "",
			toolCalls: [],
		});
		expect(result.mirroring).toBe(false);
	});

	// One finalize per stream, decided by the edge and not by the state, so a
	// steady run of idle snapshots after a mirrored reply cannot append it again.
	it("finalizes once, not on every idle tick after it", () => {
		const ended = snapshot(reply("the whole reply", 1000));
		const first = projectAiChat(ended, local({ mirroring: true }));
		const second = projectAiChat(ended, local({ mirroring: first.mirroring, mirroredFromAt: first.mirroredFromAt }));

		expect(first.finalize).toBe("the whole reply");
		expect(second.finalize).toBeNull();
		expect(second.overlay).toBeNull();
	});

	// An error ends the stream with nothing to append. The overlay still has to
	// land, or the window sits on a spinner that will never stop.
	//
	// The fixture carries a REAL previous reply, because that is what production
	// emits here: `buildAiChatSnapshot` reverse-scans the actual message list, so a
	// stream that errors after an earlier success reports that earlier reply, never
	// null. A fixture pinning `lastAssistantText: null` would assert the intention
	// in this comment into existence — it could not fail when the behaviour it
	// names breaks, because the only input that breaks it is the one it excludes.
	it("clears the overlay without appending when the stream ended in an error", () => {
		const started = projectAiChat(snapshot({ isStreaming: true, ...reply("an earlier success", 500) }), local());
		const result = projectAiChat(
			snapshot({ error: "upstream refused", ...reply("an earlier success", 500) }),
			local({ mirroring: true, mirroredFromAt: started.mirroredFromAt }),
		);

		expect(result.finalize).toBeNull();
		expect(result.overlay?.isStreaming).toBe(false);
		expect(result.overlay?.error).toBe("upstream refused");
		expect(result.mirroring).toBe(false);
	});

	// An autonomous run leaves its tool cards and text on screen by itself —
	// `textChunks` is never cleared on completion the way `streamingText` is — so
	// mirroring it needs no finalize step.
	it("mirrors an autonomous run without finalizing a message", () => {
		const running = projectAiChat(
			snapshot({ agentState: "running", textChunks: "step one" }),
			local({ mirroring: false }),
		);
		expect(running.overlay?.textChunks).toBe("step one");
		expect(running.mirroring).toBe(true);

		const done = projectAiChat(
			snapshot({ agentState: "completed", textChunks: "step one and two" }),
			local({ agentState: "running", mirroring: true }),
		);
		expect(done.overlay?.textChunks).toBe("step one and two");
		expect(done.finalize).toBeNull();
	});

	// `lastAssistantText` is the last reply in the main window's HISTORY, not the
	// reply the stream just produced. The two diverge whenever a mirrored stream
	// ends without producing one — a cancel, or an error — and finalizing on the
	// edge alone then appends the PREVIOUS reply a second time. Nothing corrects
	// it either: the mirrored reply is deliberately never persisted, so no disk
	// state contradicts the duplicate and it survives until the window re-inits.
	it("does not append the same reply again when a later stream ends without producing one", () => {
		const first = projectAiChat(snapshot({ isStreaming: true, streamingText: "A par" }), local());
		const landed = projectAiChat(
			snapshot(reply("A", 1000)),
			local({ mirroring: true, mirroredFromAt: first.mirroredFromAt }),
		);
		expect(landed.finalize).toBe("A");

		// A second stream starts and is cancelled: no new assistant message, so the
		// main window's last reply is still A.
		const restarted = projectAiChat(
			snapshot({ isStreaming: true, streamingText: "B par", ...reply("A", 1000) }),
			local({ mirroring: false, mirroredFromAt: landed.mirroredFromAt }),
		);
		const cancelled = projectAiChat(
			snapshot({ ...reply("A", 1000), error: "cancelled" }),
			local({ mirroring: true, mirroredFromAt: restarted.mirroredFromAt }),
		);

		expect(cancelled.finalize).toBeNull();
		// The overlay still has to land, or the window keeps a spinner forever.
		expect(cancelled.overlay?.isStreaming).toBe(false);
		expect(cancelled.overlay?.error).toBe("cancelled");
	});

	// The mirror image: suppressing a duplicate must not suppress a real second
	// reply. Identity is the message's timestamp, so two replies that happen to
	// read the same still both land — plausible for short watcher-driven answers
	// ("Done."), which a text comparison would silently swallow.
	it("finalizes a genuinely new reply that reads identically to the last one", () => {
		const started = projectAiChat(snapshot({ isStreaming: true, ...reply("Done.", 1000) }), local());
		const ended = projectAiChat(
			snapshot(reply("Done.", 2000)),
			local({ mirroring: true, mirroredFromAt: started.mirroredFromAt }),
		);

		expect(ended.finalize).toBe("Done.");
	});

	// The same duplicate, reached from mount instead of from a previous mirror:
	// the window loads a conversation whose last message is already an assistant
	// reply, then mirrors a stream that errors. Baselining at the START of the
	// stream is what covers both paths with one rule.
	it("does not append a reply that was already on screen when mirroring started", () => {
		const started = projectAiChat(snapshot({ isStreaming: true, ...reply("loaded from disk", 500) }), local());
		expect(started.mirroredFromAt).toBe(500);

		const errored = projectAiChat(
			snapshot({ ...reply("loaded from disk", 500), error: "upstream refused" }),
			local({ mirroring: true, mirroredFromAt: started.mirroredFromAt }),
		);

		expect(errored.finalize).toBeNull();
	});

	// The baseline is taken once, when the stream starts — not re-taken on every
	// busy tick, or the reply would always look like it was already there.
	it("keeps the baseline it took at the start of the stream", () => {
		const started = projectAiChat(snapshot({ isStreaming: true, ...reply("older", 100) }), local());
		const midway = projectAiChat(
			snapshot({ isStreaming: true, streamingText: "grow", ...reply("older", 100) }),
			local({ mirroring: true, mirroredFromAt: started.mirroredFromAt }),
		);

		expect(midway.mirroredFromAt).toBe(100);

		const ended = projectAiChat(
			snapshot(reply("the new one", 900)),
			local({ mirroring: true, mirroredFromAt: midway.mirroredFromAt }),
		);
		expect(ended.finalize).toBe("the new one");
	});

	// Nothing was handed over, so there is no conversation to project.
	it("writes nothing when the main window has no snapshot to give", () => {
		const result = projectAiChat(null, local({ mirroring: true }));

		expect(result.overlay).toBeNull();
		expect(result.finalize).toBeNull();
		expect(result.mirroring).toBe(false);
	});
});

/**
 * The cases above hand `projectAiChat` snapshots built by hand, and a hand-built
 * snapshot is free to state a combination production never emits. These drive the
 * real serializer over a real message list instead, so the two halves are checked
 * against each other rather than against a fixture's idea of the other.
 */
describe("buildAiChatSnapshot feeding projectAiChat", () => {
	const source = (messages: ConversationMessage[], over: Partial<AiChatSnapshot> = {}): AiChatSnapshotSource => {
		const base = snapshot(over);
		return {
			messages: () => messages,
			chatId: () => base.chatId,
			isStreaming: () => base.isStreaming,
			streamingText: () => base.streamingText,
			isThinking: () => base.isThinking,
			error: () => base.error,
			agentState: () => base.agentState,
			textChunks: () => base.textChunks,
			toolCalls: () => base.toolCalls,
		};
	};

	const said = (role: ConversationMessage["role"], content: string, timestamp: number): ConversationMessage => ({
		role,
		content,
		timestamp,
	});

	// The serializer reports the last reply in the HISTORY, so a history that ends
	// in one hands the projection a non-null `lastAssistantText` even when the
	// stream now running has produced nothing. That is the input every duplicate
	// path starts from, and no hand-built fixture is needed to see it.
	it("reports the previous reply while a new stream is still running", () => {
		const history = [said("user", "first question", 100), said("assistant", "first answer", 200)];
		const built = buildAiChatSnapshot(source(history, { isStreaming: true, streamingText: "wor" }));

		expect(built.lastAssistantText).toBe("first answer");
		expect(built.lastAssistantAt).toBe(200);
	});

	// The end-to-end duplicate: a second stream errors out, and the serializer
	// still reports the FIRST reply because it is still the last one in the
	// history. Finalizing on the edge alone would append it a second time.
	it("does not re-finalize the earlier reply when a later stream errors out", () => {
		const history = [said("user", "first question", 100), said("assistant", "first answer", 200)];

		const running = projectAiChat(buildAiChatSnapshot(source(history, { isStreaming: true })), local());
		expect(running.mirroring).toBe(true);

		const errored = projectAiChat(buildAiChatSnapshot(source(history, { error: "upstream refused" })), {
			...local({ mirroring: true }),
			mirroredFromAt: running.mirroredFromAt,
		});

		expect(errored.finalize).toBeNull();
		expect(errored.overlay?.error).toBe("upstream refused");
	});

	// And the reply that IS new still lands: the history grew by an assistant
	// message while the stream ran, so the timestamp moved past the baseline.
	it("finalizes the reply the mirrored stream actually produced", () => {
		const before = [said("user", "first question", 100), said("assistant", "first answer", 200)];
		const running = projectAiChat(buildAiChatSnapshot(source(before, { isStreaming: true })), local());

		const after = [...before, said("user", "second question", 300), said("assistant", "second answer", 400)];
		const ended = projectAiChat(buildAiChatSnapshot(source(after)), {
			...local({ mirroring: true }),
			mirroredFromAt: running.mirroredFromAt,
		});

		expect(ended.finalize).toBe("second answer");
	});
});
