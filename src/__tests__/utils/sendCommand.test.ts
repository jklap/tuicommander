import { describe, expect, it } from "vitest";
import {
	AGENT_ENTER_GAP_MS,
	containsShellMetacharacters,
	sendCommand,
	shouldAutoSubmitSuggestion,
} from "../../utils/sendCommand";

/**
 * Fake writer that records every call in order. Returns a resolved promise
 * so sendCommand's internal awaits don't stall.
 */
function makeRecorder() {
	const calls: string[] = [];
	const writeFn = async (data: string): Promise<void> => {
		calls.push(data);
	};
	return { writeFn, calls };
}

describe("sendCommand", () => {
	it("never sends a leading Ctrl-U prefix, for an agent session", async () => {
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "ls", "claude");
		expect(calls).toEqual(["ls", "\r"]);
	});

	it("never sends a leading Ctrl-U prefix, for a plain shell", async () => {
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "ls", null);
		expect(calls).toEqual(["ls", "\r"]);
	});

	it("wraps multi-line text in bracketed paste sequences", async () => {
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "line1\nline2", null);
		expect(calls).toEqual(["\x1b[200~line1\nline2\x1b[201~", "\r"]);
	});

	it("does not wrap single-line text in bracketed paste", async () => {
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "single line", null);
		expect(calls).toEqual(["single line", "\r"]);
	});

	it("sends Enter as a separate write", async () => {
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "foo", null);
		expect(calls.length).toBe(2);
		expect(calls[1]).toBe("\r");
	});

	it("withholds the trailing Enter when submit is false", async () => {
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "rm -rf /", null, false);
		// Text is typed but NOT executed — user must press Enter.
		expect(calls).toEqual(["rm -rf /"]);
	});

	it("submits by default (submit omitted) — backward compatible", async () => {
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "ls", null);
		expect(calls).toEqual(["ls", "\r"]);
	});

	/**
	 * Regression: two writes are not two reads. Without an elapsed-time gap the
	 * PTY coalesces payload + CR into one read() and an Ink/raw-mode agent
	 * (Codex, Claude Code) renders the CR as a newline in its composer instead
	 * of submitting — the suggestion is typed but never sent.
	 */
	it("separates the Enter from the payload in TIME when an agent is attached", async () => {
		const stamps: number[] = [];
		const writeFn = async (): Promise<void> => {
			stamps.push(performance.now());
		};
		await sendCommand(writeFn, "run the tests", "codex");
		expect(stamps.length).toBe(2);
		// setTimeout never fires early; allow a small scheduler tolerance.
		expect(stamps[1] - stamps[0]).toBeGreaterThanOrEqual(AGENT_ENTER_GAP_MS - 5);
	});

	it("does not delay the Enter on a plain shell (line-buffered, no coalescing risk)", async () => {
		const stamps: number[] = [];
		const writeFn = async (): Promise<void> => {
			stamps.push(performance.now());
		};
		await sendCommand(writeFn, "ls", null);
		expect(stamps[1] - stamps[0]).toBeLessThan(AGENT_ENTER_GAP_MS);
	});

	/**
	 * pi (0.83.0) accepts a plain `text\r` write and submits — verified live
	 * against a real pi PTY. So pi needs no special-casing: it takes the same
	 * agent path (gapped Enter, no prefix) as every other agent. This pins
	 * that — a future "optimization" that routes pi around the gap would be a
	 * silent regression on the agents that DO need it, for no gain on pi.
	 */
	it("routes pi through the standard agent path (gapped Enter, no prefix)", async () => {
		const stamps: number[] = [];
		const calls: string[] = [];
		const writeFn = async (data: string): Promise<void> => {
			calls.push(data);
			stamps.push(performance.now());
		};
		await sendCommand(writeFn, "say only the word OK", "pi");
		expect(calls).toEqual(["say only the word OK", "\r"]);
		expect(stamps[1] - stamps[0]).toBeGreaterThanOrEqual(AGENT_ENTER_GAP_MS - 5);
	});

	it("does not delay when the Enter is withheld", async () => {
		const started = performance.now();
		const { writeFn, calls } = makeRecorder();
		await sendCommand(writeFn, "run the tests", "codex", false);
		expect(calls).toEqual(["run the tests"]);
		expect(performance.now() - started).toBeLessThan(AGENT_ENTER_GAP_MS);
	});
});

describe("containsShellMetacharacters", () => {
	it("flags command chaining, substitution, and redirection", () => {
		for (const s of ["a; b", "a | b", "a && b", "$(whoami)", "`id`", "echo > f", "cat < f", "a\nb"]) {
			expect(containsShellMetacharacters(s)).toBe(true);
		}
	});

	it("does not flag plain suggestion prose", () => {
		for (const s of ["Fix the bug", "Run tests", "Deploy", "Refactor auth module"]) {
			expect(containsShellMetacharacters(s)).toBe(false);
		}
	});
});

describe("shouldAutoSubmitSuggestion", () => {
	it("always submits on an agent, even multi-line or metachar-bearing text", () => {
		for (const s of ["Fix the bug", "line one\nline two", "a; b", "$(whoami)", "echo > f"]) {
			expect(shouldAutoSubmitSuggestion("codex", s)).toBe(true);
			expect(shouldAutoSubmitSuggestion("claude", s)).toBe(true);
		}
	});

	it("withholds submit on a shell for metachar-bearing or multi-line text", () => {
		for (const agentType of [null, undefined]) {
			for (const s of ["a; b", "a | b", "$(whoami)", "echo > f", "line one\nline two"]) {
				expect(shouldAutoSubmitSuggestion(agentType, s)).toBe(false);
			}
		}
	});

	it("submits on a shell for plain single-line prose", () => {
		for (const agentType of [null, undefined]) {
			for (const s of ["Run tests", "Deploy", "Fix the bug"]) {
				expect(shouldAutoSubmitSuggestion(agentType, s)).toBe(true);
			}
		}
	});
});
