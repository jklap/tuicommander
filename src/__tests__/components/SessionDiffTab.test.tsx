import { render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
// Mocks `@tauri-apps/api/event`'s `listen` (and friends) to a no-op resolved
// stub — SessionDiffTab now subscribes to `session-review-changed`/
// `review-sessions-changed` on mount; without this, `listen()` falls through
// to invoke.ts's browser-mode SSE branch and tries to open a real
// EventSource in happy-dom.
import "../mocks/tauri";

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
					steps: Array<{ step: { tool_use_id: string; rel_path: string | null; abs_path: string } }>;
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
		onJumpToAgent?: () => void;
		wrap?: boolean;
		maxLines?: number;
		flashKeys?: ReadonlySet<string>;
		ref?: (handle: {
			scrollToIndex: (i: number) => void;
			currentIndex: () => number;
			rowCount: () => number;
			visibleIndices: () => ReadonlySet<number>;
		}) => void;
	}) => {
		props.ref?.({
			scrollToIndex: h.navScrollToIndex,
			currentIndex: () => h.navCurrentIndex,
			rowCount: () => h.navRowCount,
			visibleIndices: () => h.navVisibleIndices,
		});
		return (
			<div
				data-testid="stub-list"
				data-wrap={String(props.wrap ?? false)}
				data-max-lines={String(props.maxLines ?? 0)}
				data-flash-keys={[...(props.flashKeys ?? [])].join(",")}
			>
				{props.onJumpToAgent && (
					<button type="button" data-testid="jump-to-agent" onClick={() => props.onJumpToAgent?.()}>
						jump to agent
					</button>
				)}
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
								row.steps.map((entry) => (
									<div>
										<span data-testid="step-path">{entry.step.rel_path ?? entry.step.abs_path}</span>
										<button
											type="button"
											title="Revert just this step"
											onClick={() => props.onRevertStep(entry.step as never)}
										>
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
		);
	},
}));

const h = vi.hoisted(() => ({
	listReviewSessions: vi.fn(),
	getSessionReview: vi.fn(),
	revertSessionStep: vi.fn(),
	revertFileToSessionStart: vi.fn(),
	watchSessionReview: vi.fn().mockResolvedValue(undefined),
	unwatchSessionReview: vi.fn().mockResolvedValue(undefined),
	navScrollToIndex: vi.fn(),
	navCurrentIndex: 0,
	navRowCount: 0,
	navVisibleIndices: new Set<number>() as ReadonlySet<number>,
}));
vi.mock("../../hooks/useRepository", () => ({
	useRepository: () => ({
		listReviewSessions: h.listReviewSessions,
		getSessionReview: h.getSessionReview,
		revertSessionStep: h.revertSessionStep,
		revertFileToSessionStart: h.revertFileToSessionStart,
		watchSessionReview: h.watchSessionReview,
		unwatchSessionReview: h.unwatchSessionReview,
	}),
}));

/** `getSessionReview`'s 4th argument is the whitespace/case `DiffOptions`
 *  built from `settingsStore.state` (all-false by default — this file
 *  doesn't touch those settings, so every call site gets this same object). */
const DEFAULT_DIFF_OPTIONS = {
	ignoreLeadingWs: false,
	ignoreTrailingWs: false,
	ignoreWsAmount: false,
	ignoreCase: false,
};

import { listen as tauriListen } from "@tauri-apps/api/event";
import { SessionDiffTab } from "../../components/SessionDiffTab/SessionDiffTab";
import { diffTabsStore } from "../../stores/diffTabs";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { uiStore } from "../../stores/ui";
import type { EditStep, FileReview, SessionReview, SessionSummary } from "../../types/sessionDiff";
import { writeClipboard } from "../../utils/clipboard";
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
		tuic_session_id: null,
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
		turn_index: 0,
		turn_started_at: null,
		prompt_preview: null,
		agent_id: null,
		agent_display_name: null,
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
		revision: "rev1",
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
		tuic_session_id: null,
		turns: [],
		...overrides,
	};
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Captures every `listen(event, handler)` registration SessionDiffTab makes
 *  (via `../../invoke`'s `listen`, which calls the mocked `@tauri-apps/api/event`
 *  `listen` directly with the same handler) so a test can fire a specific
 *  event by name instead of the mock's default no-op. */
function captureListenHandlers(): Map<string, (event: { payload: unknown }) => void> {
	const handlers = new Map<string, (event: { payload: unknown }) => void>();
	vi.mocked(tauriListen).mockImplementation(((event: string, handler: (e: { payload: unknown }) => void) => {
		handlers.set(event, handler);
		return Promise.resolve(vi.fn());
	}) as unknown as typeof tauriListen);
	return handlers;
}

describe("SessionDiffTab", () => {
	let tabId: string;

	beforeEach(() => {
		h.listReviewSessions.mockReset().mockResolvedValue([summary()]);
		h.getSessionReview.mockReset().mockResolvedValue(review());
		h.revertSessionStep.mockReset();
		h.revertFileToSessionStart.mockReset();
		h.navScrollToIndex.mockReset();
		h.navCurrentIndex = 0;
		h.navRowCount = 0;
		h.navVisibleIndices = new Set();
		h.watchSessionReview.mockClear();
		h.unwatchSessionReview.mockClear();
		vi.mocked(tauriListen).mockReset().mockResolvedValue(vi.fn());
		diffTabsStore.clearAll();
		tabId = diffTabsStore.addSessionReview(REPO);
	});

	it("loads sessions and the review on mount, rendering the default grouped-by-file view", async () => {
		render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();
		expect(h.listReviewSessions).toHaveBeenCalledWith(REPO, 20, true);
		expect(h.getSessionReview).toHaveBeenCalledWith(REPO, "sess-1", true, DEFAULT_DIFF_OPTIONS);
	});

	it("defaults to the focused terminal's live agentSessionId over the newest listed session", async () => {
		h.listReviewSessions.mockResolvedValue([summary({ session_id: "sess-old" }), summary({ session_id: "sess-live" })]);
		const termId = terminalsStore.add(makeTerminal({ agentSessionId: "sess-live" }));
		terminalsStore.setActive(termId);

		render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();
		expect(h.getSessionReview).toHaveBeenCalledWith(REPO, "sess-live", true, DEFAULT_DIFF_OPTIONS);
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

	it("the live session refetches once for a burst of revision bumps (debounced, slow event-transport-down fallback)", async () => {
		vi.useFakeTimers();
		try {
			const termId = terminalsStore.add(makeTerminal({ agentSessionId: "sess-1" }));
			terminalsStore.setActive(termId);

			render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await vi.advanceTimersByTimeAsync(0);
			const initial = h.getSessionReview.mock.calls.length;

			// This revision-bump poll is now a slow (10s) fallback for when the
			// dedicated `session-review-changed` event doesn't arrive — see
			// REVIEW_REFRESH_DEBOUNCE_MS's own doc comment.
			repositoriesStore.bumpRevision(REPO);
			await vi.advanceTimersByTimeAsync(100);
			repositoriesStore.bumpRevision(REPO);
			await vi.advanceTimersByTimeAsync(100);
			repositoriesStore.bumpRevision(REPO);
			await vi.advanceTimersByTimeAsync(10_100);

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

			// Simulate the live session's debounced fallback poll refresh — same
			// session, same file, review() updates again.
			repositoriesStore.bumpRevision(REPO);
			await vi.advanceTimersByTimeAsync(10_100);

			// Must still be collapsed — a fresh `review()` for the same
			// session must not force it back open.
			expect(() => getByText("a.ts", { selector: "[data-testid=step-path]" })).toThrow();
			expect(getByText("expand")).toBeTruthy();
		} finally {
			vi.useRealTimers();
		}
	});

	it("switching the selected session clears the old session's content instead of leaving it on screen mid-fetch", async () => {
		h.listReviewSessions.mockResolvedValue([
			summary({ session_id: "sess-1", title: "First session" }),
			summary({ session_id: "sess-2", title: "Second session" }),
		]);
		let resolveSecond: (r: SessionReview) => void = () => {};
		h.getSessionReview.mockImplementation((_repo: string, sessionId: string) => {
			if (sessionId === "sess-1") return Promise.resolve(review({ session_id: "sess-1" }));
			return new Promise<SessionReview>((resolve) => {
				resolveSecond = resolve;
			});
		});

		const { getByText, getAllByText, getByTitle, queryAllByText } = render(() => (
			<SessionDiffTab tabId={tabId} repoPath={REPO} />
		));
		await settle();
		expect(getAllByText("a.ts").length).toBeGreaterThan(0);

		// Open the session picker and switch to sess-2, whose review is still pending.
		getByTitle("Choose which Claude Code session to review").click();
		await settle();
		getByText((content) => content.startsWith("Second session")).click();
		await settle();

		// sess-1's content must not still be on screen while sess-2 loads —
		// today `loadReview` only shows the loading state when `!review()`
		// (SessionDiffTab.tsx:97), so the stale review lingers until the new
		// one resolves.
		expect(queryAllByText("a.ts").length).toBe(0);

		resolveSecond(
			review({ session_id: "sess-2", files: [fileGroup({ abs_path: "/repo/z.ts", display_path: "z.ts" })] }),
		);
		await settle();
		expect(getByText("z.ts")).toBeTruthy();
	});

	it("toggling 'include subagents' re-fetches the review with the new flag", async () => {
		const { getByRole } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();
		expect(h.getSessionReview).toHaveBeenCalledWith(REPO, "sess-1", true, DEFAULT_DIFF_OPTIONS);

		const checkbox = getByRole("checkbox") as HTMLInputElement;
		checkbox.click();
		await settle();
		expect(h.getSessionReview).toHaveBeenCalledWith(REPO, "sess-1", false, DEFAULT_DIFF_OPTIONS);
	});

	it("'Copy all' copies every file's cumulative patch joined together", async () => {
		const r = review({
			files: [
				fileGroup({ abs_path: "/repo/a.ts", cumulative_patch: "PATCH_A" }),
				fileGroup({ abs_path: "/repo/b.ts", cumulative_patch: "PATCH_B" }),
			],
		});
		h.getSessionReview.mockResolvedValue(r);
		const { getByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();

		getByText("Copy all").click();
		await settle();

		expect(writeClipboard).toHaveBeenCalledWith("PATCH_A\nPATCH_B");
		expect(toastsStore.add).toHaveBeenCalledWith("Copied", expect.stringContaining("combined diff"), "info");
	});

	it("a file revert that needs force shows a toast with a 'force revert anyway' action, which retries with force:true", async () => {
		h.revertFileToSessionStart.mockResolvedValueOnce({
			applied: false,
			method: "restore_backup",
			abs_path: "/repo/a.ts",
			message: "File changed since — force?",
		});
		h.revertFileToSessionStart.mockResolvedValueOnce({
			applied: true,
			method: "restore_backup",
			abs_path: "/repo/a.ts",
			message: null,
		});
		(toastsStore.add as ReturnType<typeof vi.fn>).mockClear();

		const { container, getByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
		await settle();

		(container.querySelector('[title="Revert this file to its session-start content"]') as HTMLButtonElement).click();
		await settle();
		getByText("Revert").click();
		await settle();

		expect(h.revertFileToSessionStart).toHaveBeenCalledWith(REPO, "sess-1", "/repo/a.ts", false);
		const toastCall = (toastsStore.add as ReturnType<typeof vi.fn>).mock.calls.find(
			(c: unknown[]) => c[0] === "Can't revert — file changed since",
		);
		expect(toastCall).toBeTruthy();
		const action = toastCall?.[4] as { label: string; onClick: () => void } | undefined;
		expect(action?.label).toBe("Force revert anyway");

		action?.onClick();
		await settle();
		expect(h.revertFileToSessionStart).toHaveBeenCalledWith(REPO, "sess-1", "/repo/a.ts", true);
	});

	describe("jump to agent (review().tuic_session_id -> terminal tab)", () => {
		it("passes onJumpToAgent to the list when tuic_session_id resolves to a live terminal, and clicking it activates that terminal", async () => {
			const termId = terminalsStore.add(makeTerminal({ sessionId: "tuic-sess-1" }));
			h.getSessionReview.mockResolvedValue(review({ tuic_session_id: "tuic-sess-1" }));

			const { getByTestId } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();

			getByTestId("jump-to-agent").click();
			expect(terminalsStore.state.activeId).toBe(termId);
		});

		it("does not pass onJumpToAgent when tuic_session_id is null or matches no live terminal", async () => {
			h.getSessionReview.mockResolvedValue(review({ tuic_session_id: null }));
			const { queryByTestId } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();
			expect(queryByTestId("jump-to-agent")).toBeNull();
		});

		it("does not pass onJumpToAgent when tuic_session_id points at a terminal that no longer exists", async () => {
			h.getSessionReview.mockResolvedValue(review({ tuic_session_id: "tuic-sess-gone" }));
			const { queryByTestId } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();
			expect(queryByTestId("jump-to-agent")).toBeNull();
		});
	});

	describe("navigation: </> buttons and the turn picker", () => {
		it("the turn picker is shown only in chronological mode", async () => {
			const { getByText, queryByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();
			expect(queryByText(/^Turns \(/)).toBeNull();

			getByText("Chronological").click();
			expect(getByText(/^Turns \(/)).toBeTruthy();

			getByText("By file").click();
			expect(queryByText(/^Turns \(/)).toBeNull();
		});

		it("clicking '>' calls the list's scrollToIndex with currentIndex + 1, clamped to the last row", async () => {
			h.getSessionReview.mockResolvedValue(
				review({ files: [fileGroup({ abs_path: "/repo/a.ts" }), fileGroup({ abs_path: "/repo/b.ts" })] }),
			);
			h.navCurrentIndex = 0;
			const { getByTitle } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();

			getByTitle("Next file").click();
			expect(h.navScrollToIndex).toHaveBeenCalledWith(1, { align: "center" });

			h.navScrollToIndex.mockClear();
			h.navCurrentIndex = 999; // past the end — clamp to rows().length - 1 (2 files here).
			getByTitle("Next file").click();
			expect(h.navScrollToIndex).toHaveBeenCalledWith(1, { align: "center" });
		});

		it("clicking '<' calls scrollToIndex with currentIndex - 1, clamped to 0", async () => {
			// currentIndex starts at 1 (not 0) — a currentIndex of 0 disables the
			// "Previous file" button, and a disabled real <button> never fires
			// its click handler at all, which isn't what this test is about.
			h.getSessionReview.mockResolvedValue(
				review({ files: [fileGroup({ abs_path: "/repo/a.ts" }), fileGroup({ abs_path: "/repo/b.ts" })] }),
			);
			h.navCurrentIndex = 1;
			const { getByTitle } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();

			getByTitle("Previous file").click();
			expect(h.navScrollToIndex).toHaveBeenCalledWith(0, { align: "center" });
		});

		it("selecting a turn in the picker resolves its first step_index to a row and jumps to it", async () => {
			h.getSessionReview.mockResolvedValue(
				review({
					steps: [step({ step_index: 0, tool_use_id: "toolu_a" }), step({ step_index: 3, tool_use_id: "toolu_b" })],
					turns: [
						{
							turn_index: 0,
							started_at: null,
							prompt_preview: "First turn",
							step_indices: [0],
							additions: 1,
							deletions: 1,
							files: ["a.ts"],
						},
						{
							turn_index: 1,
							started_at: null,
							prompt_preview: "Second turn",
							step_indices: [3],
							additions: 1,
							deletions: 1,
							files: ["b.ts"],
						},
					],
				}),
			);
			const { getByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();

			getByText("Chronological").click();
			getByText("Turns (2)").click();
			getByText("b.ts").closest("button")?.click();

			// step_index 3 is chronological row 1 (steps sorted by step_index: [0, 3]).
			expect(h.navScrollToIndex).toHaveBeenCalledWith(1, { align: "center" });
		});
	});

	describe("live updates", () => {
		it("watches the selected session on mount, moves the watch on a session switch, and unwatches on unmount", async () => {
			h.listReviewSessions.mockResolvedValue([
				summary({ session_id: "sess-1", title: "First session" }),
				summary({ session_id: "sess-2", title: "Second session" }),
			]);
			h.getSessionReview.mockImplementation((_repo: string, sessionId: string) =>
				Promise.resolve(review({ session_id: sessionId })),
			);
			const { getByTitle, getByText, unmount } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();
			expect(h.watchSessionReview).toHaveBeenCalledWith(REPO, "sess-1");
			expect(h.unwatchSessionReview).not.toHaveBeenCalled();

			getByTitle("Choose which Claude Code session to review").click();
			await settle();
			getByText((content) => content.startsWith("Second session")).click();
			await settle();
			expect(h.unwatchSessionReview).toHaveBeenCalledWith(REPO, "sess-1");
			expect(h.watchSessionReview).toHaveBeenCalledWith(REPO, "sess-2");

			unmount();
			expect(h.unwatchSessionReview).toHaveBeenCalledWith(REPO, "sess-2");
		});

		it("a review-sessions-changed event for this repo refreshes the session dropdown", async () => {
			const handlers = captureListenHandlers();
			render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();
			const initial = h.listReviewSessions.mock.calls.length;

			handlers.get("review-sessions-changed")?.({ payload: { repo_path: "/some/other/repo" } });
			await settle();
			expect(h.listReviewSessions.mock.calls.length).toBe(initial); // different repo — ignored

			handlers.get("review-sessions-changed")?.({ payload: { repo_path: REPO } });
			await settle();
			expect(h.listReviewSessions.mock.calls.length).toBe(initial + 1);
		});

		it("a session-review-changed event for a different repo or session is ignored", async () => {
			const handlers = captureListenHandlers();
			render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();
			const initial = h.getSessionReview.mock.calls.length;

			handlers.get("session-review-changed")?.({ payload: { repo_path: REPO, session_id: "sess-other" } });
			await settle();
			handlers.get("session-review-changed")?.({ payload: { repo_path: "/other/repo", session_id: "sess-1" } });
			await settle();
			expect(h.getSessionReview.mock.calls.length).toBe(initial);
		});

		it("chronological mode, Follow on (default): a new step applies and auto-scrolls to the last row", async () => {
			const handlers = captureListenHandlers();
			const initialReview = review({
				steps: [step({ step_index: 0, tool_use_id: "toolu_a" })],
				files: [fileGroup({ step_indices: [0] })],
			});
			h.getSessionReview.mockResolvedValue(initialReview);
			const { getByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();
			getByText("Chronological").click();

			h.getSessionReview.mockResolvedValue(
				review({
					steps: [step({ step_index: 0, tool_use_id: "toolu_a" }), step({ step_index: 1, tool_use_id: "toolu_b" })],
					files: [fileGroup({ step_indices: [0, 1] })],
				}),
			);
			handlers.get("session-review-changed")?.({ payload: { repo_path: REPO, session_id: "sess-1" } });
			await settle();

			expect(h.navScrollToIndex).toHaveBeenCalledWith(1, { align: "end" }); // 2 rows -> last index 1
		});

		it("chronological mode, Follow off: a new step shows an 'N new changes' pill instead of auto-scrolling, and clicking it jumps to the first unseen step", async () => {
			const handlers = captureListenHandlers();
			h.getSessionReview.mockResolvedValue(
				review({
					steps: [step({ step_index: 0, tool_use_id: "toolu_a" })],
					files: [fileGroup({ step_indices: [0] })],
				}),
			);
			const { getByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();
			getByText("Chronological").click();

			// Two checkboxes render in chronological mode ("Follow" and "Subagent
			// edits") — find the Follow-labeled one specifically and turn it off.
			const followLabel = Array.from(document.querySelectorAll("label")).find((l) => l.textContent?.includes("Follow"));
			const followInput = followLabel?.querySelector("input");
			if (!followInput) throw new Error("Follow checkbox not found");
			followInput.click();
			await settle();

			h.navScrollToIndex.mockClear();
			h.getSessionReview.mockResolvedValue(
				review({
					steps: [step({ step_index: 0, tool_use_id: "toolu_a" }), step({ step_index: 1, tool_use_id: "toolu_b" })],
					files: [fileGroup({ step_indices: [0, 1] })],
				}),
			);
			handlers.get("session-review-changed")?.({ payload: { repo_path: REPO, session_id: "sess-1" } });
			await settle();

			expect(h.navScrollToIndex).not.toHaveBeenCalled(); // Follow is off — no auto-scroll
			const pill = getByText("1 new change");
			expect(pill).toBeTruthy();
			pill.click();
			expect(h.navScrollToIndex).toHaveBeenCalledWith(1, { align: "center" }); // toolu_b is chronological row 1
		});

		it("file mode: a new file appended below the current scroll position shows 'New content below', which scrolls to it on click", async () => {
			const handlers = captureListenHandlers();
			h.getSessionReview.mockResolvedValue(review({ files: [fileGroup({ abs_path: "/repo/a.ts", revision: "r1" })] }));
			const { getByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();
			// `nearBottom()` compares against the CURRENT (pre-update) row count —
			// a big list with the reviewer scrolled near the top is clearly not
			// "at the bottom", so the new file below should be flagged.
			h.navRowCount = 5;
			h.navCurrentIndex = 0;

			h.getSessionReview.mockResolvedValue(
				review({
					files: [
						fileGroup({ abs_path: "/repo/a.ts", revision: "r1" }),
						fileGroup({ abs_path: "/repo/b.ts", rel_path: "b.ts", display_path: "b.ts", revision: "r-new" }),
					],
				}),
			);
			handlers.get("session-review-changed")?.({ payload: { repo_path: REPO, session_id: "sess-1" } });
			await settle();

			const pill = getByText("New content below");
			expect(pill).toBeTruthy();
			pill.click();
			expect(h.navScrollToIndex).toHaveBeenCalledWith(1, { align: "end" });
		});

		it("file mode: a changed file that's currently visible applies immediately and flashes, fading after ~1.5s", async () => {
			vi.useFakeTimers();
			try {
				const handlers = captureListenHandlers();
				h.getSessionReview.mockResolvedValue(
					review({ files: [fileGroup({ abs_path: "/repo/a.ts", revision: "r1" })] }),
				);
				const { getByTestId } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
				await vi.advanceTimersByTimeAsync(0);
				h.navVisibleIndices = new Set([0]); // the only row is on screen

				h.getSessionReview.mockResolvedValue(
					review({ files: [fileGroup({ abs_path: "/repo/a.ts", revision: "r2", cumulative_patch: "changed" })] }),
				);
				handlers.get("session-review-changed")?.({ payload: { repo_path: REPO, session_id: "sess-1" } });
				await vi.advanceTimersByTimeAsync(0);

				expect(getByTestId("stub-list").dataset.flashKeys).toBe("/repo/a.ts");
				await vi.advanceTimersByTimeAsync(1600);
				expect(getByTestId("stub-list").dataset.flashKeys).toBe("");
			} finally {
				vi.useRealTimers();
			}
		});

		it("file mode: a changed file that's off-screen is held behind a 'Refresh (N)' pill until clicked", async () => {
			const handlers = captureListenHandlers();
			h.getSessionReview.mockResolvedValue(review({ files: [fileGroup({ abs_path: "/repo/a.ts", revision: "r1" })] }));
			const { getAllByText, getByText, queryByText } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();
			h.navVisibleIndices = new Set(); // the file is off-screen

			h.getSessionReview.mockResolvedValue(
				review({ files: [fileGroup({ abs_path: "/repo/a.ts", revision: "r2", cumulative_patch: "changed" })] }),
			);
			handlers.get("session-review-changed")?.({ payload: { repo_path: REPO, session_id: "sess-1" } });
			await settle();

			// Held back — old content still shown (both the file row's own path
			// and its nested step's path render "a.ts"), not flashed.
			expect(getAllByText("a.ts").length).toBeGreaterThan(0);
			const refreshPill = getByText("Refresh (1)");
			expect(refreshPill).toBeTruthy();

			refreshPill.click();
			await settle();
			expect(queryByText("Refresh (1)")).toBeNull();
		});

		it("mounts the shared diff-options popover in its toolbar", async () => {
			const { getByTestId } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();
			expect(getByTestId("diff-options-trigger")).toBeTruthy();
		});

		it("passes soft-wrap and the truncate-lines setting through to the list", async () => {
			uiStore.setDiffSoftWrap(true);
			settingsStore.setSessionDiffTruncateLines(150);
			try {
				const { getByTestId } = render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
				await settle();
				expect(getByTestId("stub-list").dataset.wrap).toBe("true");
				expect(getByTestId("stub-list").dataset.maxLines).toBe("150");
			} finally {
				uiStore.setDiffSoftWrap(false);
				settingsStore.setSessionDiffTruncateLines(300);
				uiStore._testCancelPendingSave();
				settingsStore._testCancelPendingSave();
			}
		});

		it("re-fetches the review when a whitespace/case diff option changes", async () => {
			render(() => <SessionDiffTab tabId={tabId} repoPath={REPO} />);
			await settle();
			const initial = h.getSessionReview.mock.calls.length;

			settingsStore.setDiffIgnoreCase(true);
			try {
				await settle();
				expect(h.getSessionReview.mock.calls.length).toBe(initial + 1);
				expect(h.getSessionReview).toHaveBeenLastCalledWith(REPO, "sess-1", true, {
					...DEFAULT_DIFF_OPTIONS,
					ignoreCase: true,
				});
			} finally {
				settingsStore.setDiffIgnoreCase(false);
				settingsStore._testCancelPendingSave();
			}
		});
	});
});
