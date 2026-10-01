import { describe, expect, it } from "vitest";
import type { ProgressFlow } from "../stores/progress";
import { agentFacts, branchFacts, compactAge, repoFacts, STALE_AFTER_DAYS, subagentRows } from "../utils/sidebarRich";

// Each case names the plausible bug it catches (#1334-b659).

const NOW = 1_800_000_000_000;
const DAY_S = 86_400;
const base = { additions: 0, deletions: 0, dirtyFiles: 0, isMerged: false, lastCommitTs: null };

describe("branchFacts", () => {
	// Catches: feeding the unix-seconds commit timestamp to a millisecond formatter, so every age reads "20000d".
	it("treats lastCommitTs as seconds", () => {
		expect(branchFacts({ ...base, lastCommitTs: NOW / 1000 - 3 * 3600 }, NOW).commitAge).toBe("3h");
	});

	// Catches: printing "↑0 ↓0" on every clean branch.
	it("omits zero ahead/behind and joins the others", () => {
		expect(branchFacts({ ...base, ahead: 0, behind: 0 }, NOW).sync).toBeNull();
		expect(branchFacts({ ...base, ahead: 2, behind: 0 }, NOW).sync).toBe("↑2");
		expect(branchFacts({ ...base, ahead: 2, behind: 1 }, NOW).sync).toBe("↑2 ↓1");
	});

	// Catches: a merged branch also flagged stale, so the chip contradicts itself.
	it("reports merged over stale and stale only past the threshold", () => {
		const old = NOW / 1000 - (STALE_AFTER_DAYS + 1) * DAY_S;
		expect(branchFacts({ ...base, lastCommitTs: old, isMerged: true }, NOW).state).toBe("merged");
		expect(branchFacts({ ...base, lastCommitTs: old }, NOW).state).toBe("stale");
		expect(branchFacts({ ...base, lastCommitTs: NOW / 1000 - (STALE_AFTER_DAYS - 1) * DAY_S }, NOW).state).toBeNull();
	});

	// Catches: an unknown commit time (null) read as the epoch, flagging every new branch stale.
	it("does not call a branch with no commit time stale", () => {
		expect(branchFacts(base, NOW).state).toBeNull();
		expect(branchFacts(base, NOW).commitAge).toBeNull();
	});

	// Catches: the lifecycle verdict being ignored when isMerged was not refreshed yet.
	it("honours a merged lifecycle verdict", () => {
		expect(branchFacts({ ...base, commitStatus: "merged" }, NOW).state).toBe("merged");
	});

	// Catches: a failed dirty inspection (null) printing as NaN or "null dirty".
	it("turns an unknown dirty count into zero", () => {
		expect(branchFacts({ ...base, dirtyFiles: null }, NOW).dirtyFiles).toBe(0);
	});
});

describe("agentFacts", () => {
	const idle = { awaitingInput: null, busy: false, agentIntent: null, currentTask: null, lastPrompt: null };

	// Catches: a pending question shown as "working" because the agent is also busy.
	it("ranks error and question above busy", () => {
		expect(agentFacts({ ...idle, busy: true, awaitingInput: "question" }, null).state).toBe("input");
		expect(agentFacts({ ...idle, busy: true, awaitingInput: "error" }, null).state).toBe("error");
		expect(agentFacts({ ...idle, busy: true }, null).state).toBe("working");
		expect(agentFacts(idle, null).state).toBe("idle");
	});

	// Catches: the stale prompt hiding the declared intent, or the task hiding both.
	it("prefers intent, then task, then last prompt", () => {
		const all = { ...idle, agentIntent: "intent", lastPrompt: "prompt" };
		expect(agentFacts(all, "task").line).toBe("intent");
		expect(agentFacts({ ...all, agentIntent: null }, "task").line).toBe("task");
		expect(agentFacts({ ...all, agentIntent: null }, null).line).toBe("prompt");
		expect(agentFacts(idle, null).line).toBeNull();
	});
});

describe("repoFacts / compactAge", () => {
	// Catches: a never-polled repo (0) rendered as "20000d ago".
	it("hides the sync age until the repo was polled", () => {
		expect(repoFacts({ currentBranch: "main", openPrs: 2, worktrees: 3, polledAt: 0 }, NOW).syncedAge).toBeNull();
		expect(
			repoFacts({ currentBranch: "main", openPrs: 2, worktrees: 3, polledAt: NOW - 2 * 3600_000 }, NOW).syncedAge,
		).toBe("2h");
	});

	// Catches: a clock skew (future timestamp) rendering a negative age.
	it("clamps future timestamps to under a minute", () => {
		expect(compactAge(NOW + 60_000, NOW)).toBe("<1m");
	});
});

describe("subagentRows", () => {
	const part = (agent: string, state: "running" | "done", pty = "s1") => ({
		id: `${pty}/${agent}`,
		kind: "subagent" as const,
		title: agent,
		state,
		parent: pty,
		toolCalls: 2,
		ptyId: pty,
		agentId: agent,
	});
	const flow = (participants: ProgressFlow["participants"], events: ProgressFlow["events"] = []): ProgressFlow => ({
		project: "/r",
		participants,
		events,
		truncated: false,
	});

	// Catches: returned subagents listed above the ones still working.
	it("puts running subagents first", () => {
		const rows = subagentRows(flow([part("a", "done"), part("b", "running")]), "s1", NOW);
		expect(rows.map((r) => r.id)).toEqual(["s1/b", "s1/a"]);
	});

	// Catches: terminal columns and other sessions' subagents leaking into the list.
	it("keeps only subagents of the given session", () => {
		const terminal = { ...part("t", "running"), kind: "terminal" as const };
		const rows = subagentRows(flow([terminal, part("a", "running", "s2"), part("b", "running")]), "s1", NOW);
		expect(rows.map((r) => r.id)).toEqual(["s1/b"]);
	});

	// Catches: the age of a returned subagent still counting from its spawn.
	it("ages a running subagent from its spawn and a returned one from its return", () => {
		const events = [
			{ id: "s1/a:spawn", kind: "subagent_spawn" as const, from: "s1", summary: "", atMs: NOW - 60 * 60_000 },
			{ id: "s1/b:spawn", kind: "subagent_spawn" as const, from: "s1", summary: "", atMs: NOW - 10 * 60_000 },
			{ id: "s1/b:return", kind: "subagent_return" as const, from: "s1/b", summary: "", atMs: NOW - 2 * 60_000 },
		];
		const rows = subagentRows(flow([part("a", "running"), part("b", "done")], events), "s1", NOW);
		expect(rows.map((r) => r.age)).toEqual(["1h", "2m"]);
	});

	// Catches: a crash or phantom rows when no flow was fetched or the terminal has no session yet.
	it("returns nothing without a flow or a session", () => {
		expect(subagentRows(undefined, "s1", NOW)).toEqual([]);
		expect(subagentRows(flow([part("a", "running")]), null, NOW)).toEqual([]);
	});
});
