import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createRepositoryRefreshCoordinator } from "../../hooks/git/createRepositoryRefreshCoordinator";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { makeTerminal } from "../helpers/store";

type Wt = { branch: string; path: string; kind: "worktree" | "main" };

function snapshot(entries: Record<string, string>): Record<string, Wt> {
	return Object.fromEntries(
		Object.entries(entries).map(([id, path]) => [
			id,
			{ branch: id, path, kind: path === "/repo" ? ("main" as const) : ("worktree" as const) },
		]),
	);
}

describe("refresh coordinator: the repo-root checkout switches branch", () => {
	const closeTerminal = vi.fn().mockResolvedValue(undefined);
	let structure: Record<string, string>;
	let refresh: (repoPath?: string) => Promise<void>;

	beforeEach(() => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-10-01T00:00:00Z"));
		closeTerminal.mockClear();
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
		structure = { main: "/repo" };
		const repo = {
			// Only the main checkout exists on disk: linked worktrees in these cases are really deleted.
			getInfo: vi.fn(async (path: string) => ({ branch: "main", is_git_repo: path === "/repo" })),
			getRepoStructure: vi.fn(async () => ({ worktree_paths: snapshot(structure), merged_branches: [] })),
			getRepoDiffStats: vi.fn().mockResolvedValue({ diff_stats: {}, last_commit_ts: {}, workspace_statuses: {} }),
			detectOrphanWorktrees: vi.fn().mockResolvedValue([]),
			assessOrphanCleanup: vi.fn().mockResolvedValue([]),
			beginOrphanCleanup: vi.fn().mockResolvedValue(undefined),
			pendingOrphanCleanupAnswer: vi.fn().mockResolvedValue(null),
			clearOrphanCleanup: vi.fn().mockResolvedValue(undefined),
			removeOrphanWorktree: vi.fn().mockResolvedValue(undefined),
			getWorkspaceLifecycle: vi.fn(),
			finalizeMergedWorktree: vi.fn(),
		};
		const c = createRepositoryRefreshCoordinator({
			repo,
			dialogs: {},
			closeTerminal,
			closeTerminalsInWorktree: vi.fn().mockResolvedValue(undefined),
			setStatusInfo: vi.fn(),
		} as unknown as Parameters<typeof createRepositoryRefreshCoordinator>[0]);
		refresh = c.refreshAllBranchStats;
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
	});

	afterEach(() => {
		vi.useRealTimers();
		repositoriesStore._testCancelPendingSave();
	});

	function rootRowWithTerminal(id: string): string {
		repositoriesStore.setWorkspace("/repo", id, { worktreePath: "/repo", kind: "main" });
		const tid = terminalsStore.add(makeTerminal({ name: id, sessionId: `s-${id}`, cwd: "/repo" }));
		repositoriesStore.addTerminalToWorkspace("/repo", id, tid);
		return tid;
	}

	const rowsAtRoot = () =>
		Object.entries(repositoriesStore.get("/repo")?.workspaces ?? {})
			.filter(([, w]) => w.worktreePath === "/repo")
			.map(([id]) => id);

	it("folds the old root row into the new one, so exactly one row sits at the repo root", async () => {
		vi.advanceTimersByTime(5_000);
		await refresh("/repo");
		const tid = rootRowWithTerminal("wip-bkup");
		repositoriesStore.setActiveWorkspace("/repo", "wip-bkup");
		vi.advanceTimersByTime(60_000);

		// HEAD of the root checkout moved wip-bkup -> wip.
		structure = { wip: "/repo" };
		await refresh("/repo");
		vi.advanceTimersByTime(60_000);
		await refresh("/repo");

		expect(rowsAtRoot()).toEqual(["wip"]);
		expect(repositoriesStore.get("/repo")?.workspaces["wip"]?.terminals).toContain(tid);
		expect(closeTerminal).not.toHaveBeenCalled();
	});

	it("does the same when the old root row's worktreePath was never recorded", async () => {
		vi.advanceTimersByTime(5_000);
		await refresh("/repo");
		const tid = terminalsStore.add(makeTerminal({ name: "old", sessionId: "s-old", cwd: "/repo" }));
		repositoriesStore.setWorkspace("/repo", "wip-bkup", { kind: "main" });
		repositoriesStore.addTerminalToWorkspace("/repo", "wip-bkup", tid);
		vi.advanceTimersByTime(60_000);

		structure = { wip: "/repo" };
		await refresh("/repo");
		vi.advanceTimersByTime(60_000);
		await refresh("/repo");

		expect(repositoriesStore.get("/repo")?.workspaces["wip-bkup"]).toBeUndefined();
		expect(repositoriesStore.get("/repo")?.workspaces["wip"]?.terminals).toContain(tid);
	});
});
