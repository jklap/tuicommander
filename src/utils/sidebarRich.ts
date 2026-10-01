import { type Accessor, createSignal, onCleanup } from "solid-js";
import type { ProgressFlow } from "../stores/progress";

/**
 * Facts the rich sidebar prints under a row. Everything here is a pure read of
 * data the stores already hold; nothing calls the backend.
 */

/** A branch with no commit for this long, and not merged, reads as stale. */
export const STALE_AFTER_DAYS = 30;

const MINUTE = 60_000;

/** `<1m`, `5m`, `3h`, `12d`. `nowMs` and `thenMs` are both milliseconds. */
export function compactAge(thenMs: number | null | undefined, nowMs: number): string {
	if (!thenMs) return "";
	const minutes = Math.max(0, Math.floor((nowMs - thenMs) / MINUTE));
	if (minutes < 1) return "<1m";
	if (minutes < 60) return `${minutes}m`;
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return `${hours}h`;
	return `${Math.floor(hours / 24)}d`;
}

export interface BranchFactsInput {
	/** Unix seconds, as `lastCommitTs` holds it. */
	lastCommitTs: number | null;
	ahead?: number;
	behind?: number;
	additions: number;
	deletions: number;
	dirtyFiles: number | null | undefined;
	isMerged: boolean;
	commitStatus?: string;
}

export interface BranchFacts {
	commitAge: string | null;
	/** `↑2 ↓1`, either half omitted when zero; null when both are zero. */
	sync: string | null;
	additions: number;
	deletions: number;
	dirtyFiles: number;
	state: "merged" | "stale" | null;
}

export function branchFacts(i: BranchFactsInput, nowMs: number): BranchFacts {
	const commitMs = i.lastCommitTs ? i.lastCommitTs * 1000 : null;
	const merged = i.isMerged || i.commitStatus === "merged";
	const stale = !merged && commitMs !== null && nowMs - commitMs > STALE_AFTER_DAYS * 24 * 60 * MINUTE;
	const sync = [i.ahead ? `↑${i.ahead}` : "", i.behind ? `↓${i.behind}` : ""].filter(Boolean).join(" ");
	return {
		commitAge: commitMs ? compactAge(commitMs, nowMs) : null,
		sync: sync || null,
		additions: i.additions,
		deletions: i.deletions,
		dirtyFiles: i.dirtyFiles ?? 0,
		state: merged ? "merged" : stale ? "stale" : null,
	};
}

export type AgentRowState = "working" | "idle" | "input" | "error";

export interface AgentFactsInput {
	awaitingInput: "question" | "error" | string | null;
	busy: boolean;
	agentIntent: string | null;
	currentTask: string | null;
	lastPrompt: string | null;
}

export interface AgentFacts {
	state: AgentRowState;
	/** One line of what the agent is doing or was last asked; null when none is known. */
	line: string | null;
}

/** `task` is the already-displayable current task (see `displayTask`). */
export function agentFacts(i: AgentFactsInput, task: string | null): AgentFacts {
	const state: AgentRowState =
		i.awaitingInput === "error" ? "error" : i.awaitingInput === "question" ? "input" : i.busy ? "working" : "idle";
	return {
		state,
		line: i.agentIntent ?? task ?? i.lastPrompt ?? null,
	};
}

export interface RepoFactsInput {
	currentBranch: string | null;
	openPrs: number;
	worktrees: number;
	/** Milliseconds of the last GitHub/remote poll, 0 when never polled. */
	polledAt: number;
}

export interface RepoFacts {
	currentBranch: string | null;
	openPrs: number;
	worktrees: number;
	syncedAge: string | null;
}

export function repoFacts(i: RepoFactsInput, nowMs: number): RepoFacts {
	return {
		currentBranch: i.currentBranch,
		openPrs: i.openPrs,
		worktrees: i.worktrees,
		syncedAge: i.polledAt ? compactAge(i.polledAt, nowMs) : null,
	};
}

/** Wall clock that ticks once a minute, for ages that must not freeze on screen. */
export function createMinuteClock(): Accessor<number> {
	const [now, setNow] = createSignal(Date.now());
	const timer = setInterval(() => setNow(Date.now()), MINUTE);
	onCleanup(() => clearInterval(timer));
	return now;
}

/** More subagents than this collapse into one "N subagents" line. */
export const SUBAGENT_COLLAPSE_AFTER = 3;

export interface SubagentRow {
	id: string;
	title: string;
	running: boolean;
	toolCalls: number;
	/** Compact age: since spawn while running, since the return once done. */
	age: string;
}

/**
 * The in-session subagents of the agent running in PTY session `sessionId`,
 * running ones first. Age comes from the flow's spawn and return events; a
 * subagent with neither has none.
 */
export function subagentRows(flow: ProgressFlow | undefined, sessionId: string | null, nowMs: number): SubagentRow[] {
	if (!flow || !sessionId) return [];
	const at = new Map(flow.events.map((e) => [e.id, e.atMs]));
	const rows = flow.participants
		.filter((p) => p.kind === "subagent" && p.ptyId === sessionId)
		.map((p) => {
			const running = p.state === "running";
			return {
				id: p.id,
				title: p.title,
				running,
				toolCalls: p.toolCalls,
				age: compactAge(at.get(`${p.id}:${running ? "spawn" : "return"}`), nowMs),
			};
		});
	return [...rows.filter((r) => r.running), ...rows.filter((r) => !r.running)];
}
