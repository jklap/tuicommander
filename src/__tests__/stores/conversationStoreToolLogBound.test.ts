import { beforeEach, describe, expect, it, vi } from "vitest";

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke: mockInvoke }));

import { conversationStore } from "../../stores/conversationStore";

/**
 * The persisted tool-call log must stay bounded during a long run (#718-aebf).
 *
 * Measured before this bound existed, against the exact shape
 * `ai_chat.rs save_conversation` writes (`serde_json::to_string_pretty`): one
 * entry at the backend's 8192-byte output cap costs 8574 bytes on disk, so the
 * 500-entry count cap alone allowed 4.13 MB, rewritten whole on a 500 ms
 * debounce — up to 8.26 MB/s for as long as the run lasted.
 *
 * The assertions below serialize the way the file is serialized, so what they
 * pin is close to what lands on disk rather than a proxy for it. Two ceilings,
 * because they are two different quantities: the store's policy number counts
 * each entry on its own, and the document costs a few percent more for the
 * indentation nesting adds.
 */

/** `serde_json::to_string_pretty` — two-space indent, same as this. */
const persistedBytes = (calls: unknown[]) => JSON.stringify(calls, null, 2).length;

/** The store's ceiling: the sum of the entries' own pretty-printed sizes. */
const MAX_TOOL_CALL_BYTES = 512 * 1024;

/**
 * What that ceiling costs once the entries sit inside a document.
 *
 * Nesting indents every line of every entry further, which the per-entry sum
 * cannot see, so the file is a few percent bigger than the policy number. This
 * is the figure that actually hits the disk, and it is asserted separately so a
 * change to either the ceiling or the shape of an entry has to face both.
 */
const MAX_ON_DISK_BYTES = 560 * 1024;

/** Output at the backend's per-result cap: the worst case the store can receive. */
const cappedOutput = "x".repeat(8192);

function driveToolCall(output: string, name = "ai_terminal_read"): void {
	conversationStore.processEvent({ type: "tool_call", session_id: "s1", tool_name: name, args: { lines: 200 } });
	conversationStore.processEvent({ type: "tool_result", session_id: "s1", tool_name: name, success: true, output });
}

describe("persisted tool-call log stays bounded", () => {
	beforeEach(() => {
		conversationStore.reset();
	});

	it("holds the byte ceiling across a run long enough to blow past it", () => {
		// 500 entries at the output cap were 4.13 MB before the bound — eight
		// times the ceiling, so this run genuinely exercises it rather than
		// stopping just short.
		for (let i = 0; i < 500; i++) driveToolCall(cappedOutput);

		const calls = conversationStore.toolCalls();
		expect(persistedBytes(calls)).toBeLessThanOrEqual(MAX_ON_DISK_BYTES);
		expect(calls.length).toBeLessThan(500);
	});

	// Criterion 3: whatever is dropped is dropped from the OLDEST end. A log
	// trimmed from the wrong end would still satisfy the byte bound while
	// telling the user nothing about where the run got to.
	it("keeps the most recent activity and drops the oldest", () => {
		for (let i = 0; i < 200; i++) driveToolCall(cappedOutput, `tool_${i}`);

		const names = conversationStore.toolCalls().map((c) => c.toolName);
		expect(names[names.length - 1]).toBe("tool_199");
		expect(names).not.toContain("tool_0");
		// Contiguous from wherever it starts: a gap would mean something other
		// than an oldest-end trim happened.
		const first = Number(names[0].slice("tool_".length));
		expect(names).toEqual(Array.from({ length: names.length }, (_, i) => `tool_${first + i}`));
	});

	// The newest entry is the one the user needs; losing it to the very rule
	// meant to keep the log useful would be the worst outcome of the bound.
	it("never drops the newest entry even when it alone exceeds the ceiling", () => {
		driveToolCall("small", "tool_old");
		// A single result bigger than the whole ceiling. The backend caps results
		// at 8 KB so this cannot arrive in practice today, but the bound must not
		// depend on that other cap staying where it is.
		driveToolCall("y".repeat(MAX_TOOL_CALL_BYTES * 2), "tool_huge");

		const calls = conversationStore.toolCalls();
		expect(calls).toHaveLength(1);
		expect(calls[0].toolName).toBe("tool_huge");
	});

	// The ordinary case must be untouched: the ceiling was sized so that a run
	// whose outputs average under about 1 KB keeps every one of its 500 entries.
	it("leaves a run with ordinary-sized output completely alone", () => {
		for (let i = 0; i < 300; i++) driveToolCall("ok\n".repeat(60), `tool_${i}`);

		const calls = conversationStore.toolCalls();
		expect(calls).toHaveLength(300);
		expect(persistedBytes(calls)).toBeLessThan(MAX_TOOL_CALL_BYTES);
	});

	// Restoring goes through the same bound. A document written by a build older
	// than this cap — or hand-edited — must not put an unbounded log back into
	// memory, where the next save would write it straight back out.
	it("applies the bound to a log read back off disk", async () => {
		mockInvoke.mockResolvedValueOnce({
			meta: { id: "c1", title: "t", created: 1, updated: 2, message_count: 0 },
			messages: [],
			schema_version: 3,
			agent: {
				state: "running",
				currentIteration: 40,
				toolCalls: Array.from({ length: 500 }, (_, i) => ({
					status: "done",
					toolName: `tool_${i}`,
					args: {},
					startedAt: 1,
					result: { success: true, output: cappedOutput },
					duration: 2,
				})),
			},
		});

		await conversationStore.loadConversation("c1");

		const calls = conversationStore.toolCalls();
		expect(persistedBytes(calls)).toBeLessThanOrEqual(MAX_ON_DISK_BYTES);
		expect(calls[calls.length - 1].toolName).toBe("tool_499");
	});
});
