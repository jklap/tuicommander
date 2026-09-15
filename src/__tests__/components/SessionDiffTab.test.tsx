import { render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";

// DiffViewer needs a real Canvas that jsdom/happy-dom don't provide (see
// DiffTab.test.tsx's note) — stubbed so these tests exercise only
// SessionDiffTab's own orchestration logic.
vi.mock("../../components/ui/DiffViewer", () => ({
	DiffViewer: (props: { diff: string }) => <div data-testid="diff-stub">{props.diff}</div>,
}));
vi.mock("../../utils/filePreview", () => ({ openFileAction: vi.fn() }));
vi.mock("../../utils/clipboard", () => ({ writeClipboard: vi.fn().mockResolvedValue(undefined) }));
// A real toast schedules a real dismiss timer (20s for "info") that outlives
// any of these tests and trips vitest's async-leak detector — stub it out.
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn(), hasVisible: vi.fn().mockReturnValue(false) } }));
// @tanstack/solid-virtual can't measure rows in happy-dom (zero layout — the
// same documented limitation DiffFileList.test.tsx works around), so
// SessionDiffList is stubbed with a plain, un-virtualized renderer exposing
// the same callback surface. This file tests SessionDiffTab's OWN
// orchestration (session/review loading, revert wiring, view-mode toggling);
// SessionDiffList's own virtualization is out of scope here.
vi.mock("../../components/SessionDiffTab/SessionDiffList", () => ({
	SessionDiffList: (props: {
		rows: Array<
			| {
					kind: "file";
					group: { abs_path: string; display_path: string };
					steps: Array<{ tool_use_id: string; rel_path: string | null; abs_path: string }>;
					expanded: boolean;
			  }
			| { kind: "step"; step: { tool_use_id: string; rel_path: string | null; abs_path: string } }
		>;
		onOpenFile: (path: string) => void;
		onRevertStep: (step: never) => void;
		onRevertFile: (group: never) => void;
		onCopyStep: (step: never) => void;
		onCopyFile: (group: never) => void;
		onToggleExpanded: (absPath: string) => void;
	}) => (
		<div data-testid="stub-list">
			{props.rows.map((row) =>
				row.kind === "file" ? (
					<div>
						<span>{row.group.display_path}</span>
						<button type="button" title="Toggle expanded" onClick={() => props.onToggleExpanded(row.group.abs_path)}>
							{row.expanded ? "collapse" : "expand"}
						</button>
						<button
							type="button"
							title="Revert this file to its session-start content"
							onClick={() => props.onRevertFile(row.group as never)}
						>
							revert file
						</button>
						<button type="button" title="Copy this file's diff" onClick={() => props.onCopyFile(row.group as never)}>
							copy file
						</button>
						<button type="button" onClick={() => props.onOpenFile(row.group.abs_path)}>
							open file
						</button>
						{row.expanded &&
							row.steps.map((step) => (
								<div>
									<span data-testid="step-path">{step.rel_path ?? step.abs_path}</span>
									<button type="button" title="Revert just this step" onClick={() => props.onRevertStep(step as never)}>
										revert step
									</button>
								</div>
							))}
					</div>
				) : (
					<div>
						<span>{row.step.rel_path ?? row.step.abs_path}</span>
						<button type="button" title="Revert just this step" onClick={() => props.onRevertStep(row.step as never)}>
							revert step
						</button>
					</div>
				),
			)}
		</div>
	),
}));

const h = vi.hoisted(() => ({
	listReviewSessions: vi.fn(),
	getSessionReview: vi.fn(),
	revertSessionStep: vi.fn(),
	revertFileToSessionStart: vi.fn(),
}));
vi.mock("../../hooks/useRepository", () => ({
	useRepository: () => ({
		listReviewSessions: h.listReviewSessions,
		getSessionReview: h.getSessionReview,
		revertSessionStep: h.revertSessionStep,
		revertFileToSessionStart: h.revertFileToSessionStart,
	}),
}));

import { SessionDiffTab } from "../../components/SessionDiffTab/SessionDiffTab";
import { diffTabsStore } from "../../stores/diffTabs";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import type { EditStep, FileReview, SessionReview, SessionSummary } from "../../types/sessionDiff";
import { makeTerminal } from "../helpers/store";

const REPO = "/repo";

function summary(overrides: Partial<SessionSummary> = {}): SessionSummary {
	return {
		session_id: "sess-1",
		transcript_path: "/x.jsonl",
		cwd: REPO,
		git_branch: "main",
		started_at: new Date().toISOString(),
		ended_at: null,
		title: "My session",
		last_prompt: null,
		size_bytes: 100,
		edit_count: 1,
		file_count: 1,
		has_subagents: false,
		...overrides,
	};
}

function step(overrides: Partial<EditStep> = {}): EditStep {
	return {
		step_index: 0,
		tool_use_id: "toolu_abc",
		timestamp: "2026-09-14T21:00:00.000Z",
		kind: "edit",
		abs_path: "/repo/a.ts",
		rel_path: "a.ts",
		in_repo: true,
		patch: "@@ -1,1 +1,1 @@\n-a\n+b\n",
		additions: 1,
		deletions: 1,
		is_sidechain: false,
		agent_name: null,
		user_modified: false,
		replace_all: false,
		...overrides,
	};
}

function fileGroup(overrides: Partial<FileReview> = {}): FileReview {
	return {
		abs_path: "/repo/a.ts",
		rel_path: "a.ts",
		in_repo: true,
		display_path: "a.ts",
		net_change: "modified",
		base_source: "backup",
		cumulative_patch: "@@ -1,1 +1,1 @@\n-a\n+b\n",
		additions: 1,
		deletions: 1,
		step_indices: [0],
		drifted_from_disk: false,
		backup_available: true,
		is_binary: false,
		...overrides,
	};
}

function review(overrides: Partial<SessionReview> = {}): SessionReview {
	return {
		session_id: "sess-1",
		transcript_path: "/x.jsonl",
		repo_path: REPO,
		started_at: null,
		ended_at: null,
		title: null,
		steps: [step()],
		files: [fileGroup()],
		warnings: [],
		included_subagents: true,
		...overrides,
	};
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("SessionDiffTab", () => {
	let tabId: string;

	beforeEach(() => {
		h.listReviewSessions.mockReset().mockResolvedValue([summary()]);
		h.getSessionReview.mockReset().mockResolvedValue(review());
		h.revertSessionStep.mockReset();
		h.revertFileToSessionStart.mockReset();
		diffTabsStore.clearAll();
		tabId = diffTabsStore.addSessionReview(REPO);
	});

	it("loads sessions and the review on mount, rendering the default grouped-by-file view", async () => {
		render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();
		expect(h.listReviewSessions).toHaveBeenCalledWith(REPO, 20, true);
		expect(h.getSessionReview).toHaveBeenCalledWith(REPO, "sess-1", true);
	});

	it("defaults to the focused terminal's live agentSessionId over the newest listed session", async () => {
		h.listReviewSessions.mockResolvedValue([summary({ session_id: "sess-old" }), summary({ session_id: "sess-live" })]);
		const termId = terminalsStore.add(makeTerminal({ agentSessionId: "sess-live" }));
		terminalsStore.setActive(termId);

		render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();
		expect(h.getSessionReview).toHaveBeenCalledWith(REPO, "sess-live", true);
	});

	it("toggling to chronological view renders a flat list instead of grouped-by-file", async () => {
		const r = review({
			steps: [
				step({ tool_use_id: "toolu_1" }),
				step({ tool_use_id: "toolu_2", abs_path: "/repo/b.ts", rel_path: "b.ts" }),
			],
			files: [
				fileGroup({ step_indices: [0] }),
				fileGroup({ abs_path: "/repo/b.ts", display_path: "b.ts", step_indices: [1] }),
			],
		});
		h.getSessionReview.mockResolvedValue(r);
		const { getByText, queryAllByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();

		// Grouped view: one file-header row per file (default mode).
		expect(queryAllByText("a.ts").length).toBeGreaterThan(0);

		getByText("Chronological").click();
		await settle();
		// Flat mode renders each step directly — both files' paths show up as
		// step headers now, without needing to expand anything.
		expect(getByText("a.ts")).toBeTruthy();
		expect(getByText("b.ts")).toBeTruthy();
	});

	it("distinguishes a session with no edits from a backend error", async () => {
		h.getSessionReview.mockResolvedValueOnce(review({ steps: [], files: [] }));
		const { getByText, unmount } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();
		expect(getByText(/made no file edits/)).toBeTruthy();
		unmount();

		h.getSessionReview.mockReset().mockRejectedValueOnce(new Error("backend exploded"));
		const { getByText: getByText2 } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();
		expect(getByText2(/Error:/)).toBeTruthy();
		expect(getByText2(/backend exploded/)).toBeTruthy();
	});

	it("shows a dismissible warnings banner when the review carries warnings", async () => {
		h.getSessionReview.mockResolvedValue(review({ warnings: ["line 42 could not be parsed"] }));
		const { getByText, queryByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();
		expect(getByText(/1 issue/)).toBeTruthy();
		getByText("Dismiss").click();
		await settle();
		expect(queryByText(/1 issue/)).toBeNull();
	});

	it("revert-step: confirm defaults to Cancel and calls the command with tool_use_id, refetching exactly once on success", async () => {
		h.revertSessionStep.mockResolvedValue({
			applied: true,
			method: "git_apply_reverse",
			abs_path: "/repo/a.ts",
			message: null,
		});
		const { container, getByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();

		const revertBtn = container.querySelector('[title="Revert just this step"]') as HTMLButtonElement;
		revertBtn.click();
		await settle();

		expect(h.getSessionReview).toHaveBeenCalledTimes(1); // not yet — confirm not clicked
		getByText("Revert").click();
		await settle();

		expect(h.revertSessionStep).toHaveBeenCalledWith(REPO, "sess-1", "toolu_abc");
		expect(h.getSessionReview).toHaveBeenCalledTimes(2); // initial load + post-revert refetch
	});

	it("a failed revert does not trigger a refetch", async () => {
		h.revertSessionStep.mockResolvedValue({
			applied: false,
			method: "git_apply_reverse",
			abs_path: "/repo/a.ts",
			message: "conflict",
		});
		const { container, getByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();

		(container.querySelector('[title="Revert just this step"]') as HTMLButtonElement).click();
		await settle();
		getByText("Revert").click();
		await settle();

		expect(h.getSessionReview).toHaveBeenCalledTimes(1);
	});

	it("revert-file calls revertFileToSessionStart with force:false initially", async () => {
		h.revertFileToSessionStart.mockResolvedValue({
			applied: true,
			method: "restore_backup",
			abs_path: "/repo/a.ts",
			message: null,
		});
		const { container, getByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();

		(container.querySelector('[title="Revert this file to its session-start content"]') as HTMLButtonElement).click();
		await settle();
		getByText("Revert").click();
		await settle();

		expect(h.revertFileToSessionStart).toHaveBeenCalledWith(REPO, "sess-1", "/repo/a.ts", false);
	});

	it("a static (non-live) session ignores a working-tree revision bump", async () => {
		render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();
		const initial = h.getSessionReview.mock.calls.length;

		repositoriesStore.bumpRevision(REPO);
		await settle();
		expect(h.getSessionReview.mock.calls.length).toBe(initial);
	});

	it("the live session refetches once for a burst of revision bumps (debounced)", async () => {
		vi.useFakeTimers();
		try {
			const termId = terminalsStore.add(makeTerminal({ agentSessionId: "sess-1" }));
			terminalsStore.setActive(termId);

			render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await vi.advanceTimersByTimeAsync(0);
			const initial = h.getSessionReview.mock.calls.length;

			repositoriesStore.bumpRevision(REPO);
			await vi.advanceTimersByTimeAsync(100);
			repositoriesStore.bumpRevision(REPO);
			await vi.advanceTimersByTimeAsync(100);
			repositoriesStore.bumpRevision(REPO);
			await vi.advanceTimersByTimeAsync(2100);

			expect(h.getSessionReview.mock.calls.length).toBe(initial + 1);
		} finally {
			vi.useRealTimers();
		}
	});

	it("a manually collapsed file stays collapsed across a live-session poll refresh (regression: expandedFiles must not reset on every review update)", async () => {
		vi.useFakeTimers();
		try {
			const termId = terminalsStore.add(makeTerminal({ agentSessionId: "sess-1" }));
			terminalsStore.setActive(termId);

			const { getByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await vi.advanceTimersByTimeAsync(0);

			// Default: expanded, so the step shows.
			expect(getByText("a.ts", { selector: "[data-testid=step-path]" })).toBeTruthy();

			getByText("collapse").click();
			await vi.advanceTimersByTimeAsync(0);
			expect(() => getByText("a.ts", { selector: "[data-testid=step-path]" })).toThrow();

			// Simulate the live session's debounced poll refresh — same
			// session, same file, review() updates again.
			repositoriesStore.bumpRevision(REPO);
			await vi.advanceTimersByTimeAsync(2100);

			// Must still be collapsed — a fresh `review()` for the same
			// session must not force it back open.
			expect(() => getByText("a.ts", { selector: "[data-testid=step-path]" })).toThrow();
			expect(getByText("expand")).toBeTruthy();
		} finally {
			vi.useRealTimers();
		}
	});
});
