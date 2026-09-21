import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BranchItem, BranchTabList } from "../../../components/Sidebar/RepoSection";
import { repositoriesStore } from "../../../stores/repositories";
import { settingsStore } from "../../../stores/settings";
import { terminalsStore } from "../../../stores/terminals";
import type { WorkspaceState } from "../../../stores/workspaceIdentity";

function makeBranch(overrides: Partial<WorkspaceState> = {}): WorkspaceState {
	return {
		workspaceId: "feat-x",
		branchName: "feat-x",
		kind: "worktree",
		parentRepoPath: "/repo",
		isMain: false,
		worktreePath: "/repo/.worktrees/feat-x",
		terminals: [],
		hadTerminals: false,
		lastActiveTerminal: null,
		additions: 0,
		deletions: 0,
		isMerged: false,
		lastCommitTs: null,
		...overrides,
	};
}

const noop = () => {};
function renderBranchItem(branch: WorkspaceState) {
	return render(() => (
		<BranchItem
			branch={branch}
			repoPath="/repo"
			isActive={false}
			canRemove={true}
			onSelect={noop}
			onAddTerminal={noop}
			onRemove={noop}
			onRename={noop}
			onShowPrDetail={noop}
		/>
	));
}

describe("RepoSection", () => {
	beforeEach(() => {
		// Every test's poll-on-mount hits this — default to "nothing to reconcile"
		// so tests that don't care about the poll aren't affected by it.
		vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, json: async () => ({}) }));
	});

	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
	});

	describe("BranchTabList dot (existing indicator, baseline coverage)", () => {
		afterEach(() => {
			terminalsStore.remove("dot-idle");
			terminalsStore.remove("dot-busy");
			terminalsStore.remove("dot-error");
			terminalsStore.remove("dot-question");
			terminalsStore.remove("dot-unseen");
		});

		it("renders no dot variant class for a plain running terminal", () => {
			terminalsStore.register("dot-idle", {
				name: "shell",
				cwd: "/repo",
				sessionId: null,
				fontSize: 13,
				awaitingInput: null,
			});
			const { container } = render(() => <BranchTabList terminalIds={["dot-idle"]} />);
			const dot = container.querySelector("span[aria-hidden='true']");
			expect(dot?.className).not.toMatch(/branchTabDot(Error|Question|Busy|Unseen|Idle)/);
		});

		it("shows the error variant when the terminal is awaiting input on an error", () => {
			terminalsStore.register("dot-error", {
				name: "shell",
				cwd: "/repo",
				sessionId: null,
				fontSize: 13,
				awaitingInput: null,
			});
			terminalsStore.setAwaitingInput("dot-error", "error");
			const { container } = render(() => <BranchTabList terminalIds={["dot-error"]} />);
			const dot = container.querySelector("span[aria-hidden='true']");
			expect(dot?.className).toMatch(/branchTabDotError/);
		});

		it("shows the question variant when the terminal is awaiting a question", () => {
			terminalsStore.register("dot-question", {
				name: "shell",
				cwd: "/repo",
				sessionId: null,
				fontSize: 13,
				awaitingInput: null,
			});
			terminalsStore.setAwaitingInput("dot-question", "question");
			const { container } = render(() => <BranchTabList terminalIds={["dot-question"]} />);
			const dot = container.querySelector("span[aria-hidden='true']");
			expect(dot?.className).toMatch(/branchTabDotQuestion/);
		});

		it("shows the unseen variant when the terminal has unread output", () => {
			terminalsStore.register("dot-unseen", {
				name: "shell",
				cwd: "/repo",
				sessionId: null,
				fontSize: 13,
				awaitingInput: null,
			});
			terminalsStore.update("dot-unseen", { unseen: true });
			const { container } = render(() => <BranchTabList terminalIds={["dot-unseen"]} />);
			const dot = container.querySelector("span[aria-hidden='true']");
			expect(dot?.className).toMatch(/branchTabDotUnseen/);
		});

		it("renders nothing for a terminal id the store doesn't know about", () => {
			const { container } = render(() => <BranchTabList terminalIds={["missing-id"]} />);
			expect(container.querySelector("button")).toBeNull();
		});
	});

	describe("gitOpBadge (existing indicator, baseline coverage)", () => {
		it("shows the rebasing badge with its tooltip when a rebase is in progress", () => {
			settingsStore.setShowGitState(true);
			const { container } = renderBranchItem(makeBranch({ gitOp: "rebase" }));
			const badge = container.querySelector(".gitOpBadge");
			expect(badge?.textContent).toBe("Rebasing");
			expect(badge?.getAttribute("title")).toContain("rebasing in progress");
		});

		it("shows nothing when there is no git operation in progress", () => {
			settingsStore.setShowGitState(true);
			const { container } = renderBranchItem(makeBranch({ gitOp: null }));
			expect(container.querySelector(".gitOpBadge")).toBeNull();
		});

		it("is hidden entirely when showGitState is off, even mid-rebase", () => {
			settingsStore.setShowGitState(false);
			const { container } = renderBranchItem(makeBranch({ gitOp: "rebase" }));
			expect(container.querySelector(".gitOpBadge")).toBeNull();
			settingsStore.setShowGitState(true);
		});
	});

	describe("warmBadge (background worktree-warming indicator)", () => {
		it("shows a warming badge with a progress tooltip", () => {
			const { container } = renderBranchItem(
				makeBranch({ warmState: { status: "warming", copied: 2, total: 5, current: "node_modules" } }),
			);
			const badge = container.querySelector(".warmBadge");
			expect(badge?.textContent).toBe("Warming…");
			expect(badge?.getAttribute("title")).toBe("Warming build caches: 2/5 (node_modules)");
		});

		it("omits the current-directory clause in the tooltip before the first progress tick", () => {
			const { container } = renderBranchItem(makeBranch({ warmState: { status: "warming", copied: 0, total: 5 } }));
			const badge = container.querySelector(".warmBadge");
			expect(badge?.getAttribute("title")).toBe("Warming build caches: 0/5");
		});

		it("shows no badge once warming has completed (warmState cleared to null)", () => {
			const { container } = renderBranchItem(makeBranch({ warmState: null }));
			expect(container.querySelector(".warmBadge")).toBeNull();
		});

		it("shows no badge when nothing is warming (warmState absent)", () => {
			const { container } = renderBranchItem(makeBranch({}));
			expect(container.querySelector(".warmBadge")).toBeNull();
		});

		it("reconciles a missed event via a one-shot poll on mount", async () => {
			vi.stubGlobal(
				"fetch",
				vi.fn().mockResolvedValue({
					ok: true,
					json: async () => ({ state: "running", copied: 1, total: 3 }),
				}),
			);
			const setWorkspace = vi.spyOn(repositoriesStore, "setWorkspace").mockImplementation(() => {});

			renderBranchItem(makeBranch({ warmState: undefined, worktreePath: "/repo/.worktrees/feat-x" }));
			await vi.waitFor(() => {
				expect(setWorkspace).toHaveBeenCalledWith("/repo", "feat-x", {
					warmState: { status: "warming", copied: 1, total: 3 },
				});
			});

			setWorkspace.mockRestore();
		});

		it("does not poll for the main checkout (no worktreePath)", async () => {
			const fetchSpy = vi.fn().mockResolvedValue({ ok: false, json: async () => ({}) });
			vi.stubGlobal("fetch", fetchSpy);

			renderBranchItem(makeBranch({ worktreePath: null, isMain: true, kind: "main" }));
			await Promise.resolve();

			expect(fetchSpy).not.toHaveBeenCalled();
		});

		it("does not poll when a live warmState already arrived before mount", async () => {
			const fetchSpy = vi.fn().mockResolvedValue({ ok: false, json: async () => ({}) });
			vi.stubGlobal("fetch", fetchSpy);

			renderBranchItem(makeBranch({ warmState: { status: "warming", copied: 0, total: 1 } }));
			await Promise.resolve();

			expect(fetchSpy).not.toHaveBeenCalled();
		});

		it("clears a warming badge on its own if it outlives the safety-net timeout", () => {
			vi.useFakeTimers();
			const setWorkspace = vi.spyOn(repositoriesStore, "setWorkspace").mockImplementation(() => {});

			renderBranchItem(makeBranch({ warmState: { status: "warming", copied: 0, total: 1 } }));
			expect(setWorkspace).not.toHaveBeenCalled();

			vi.advanceTimersByTime(900_000);

			expect(setWorkspace).toHaveBeenCalledWith("/repo", "feat-x", { warmState: null });
			setWorkspace.mockRestore();
			vi.useRealTimers();
		});

		it("does not fire the safety net early, before the timeout elapses", () => {
			vi.useFakeTimers();
			const setWorkspace = vi.spyOn(repositoriesStore, "setWorkspace").mockImplementation(() => {});

			renderBranchItem(makeBranch({ warmState: { status: "warming", copied: 0, total: 1 } }));
			vi.advanceTimersByTime(899_999);

			expect(setWorkspace).not.toHaveBeenCalled();
			setWorkspace.mockRestore();
			vi.useRealTimers();
		});
	});
});
