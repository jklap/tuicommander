import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScopeAsync } from "../helpers/store";

describe("createRepositoryRefreshCoordinator", () => {
	let createRepositoryRefreshCoordinator: typeof import("../../hooks/git/createRepositoryRefreshCoordinator").createRepositoryRefreshCoordinator;
	let repositoriesStore: typeof import("../../stores/repositories").repositoriesStore;
	let terminalsStore: typeof import("../../stores/terminals").terminalsStore;
	let globalWorkspaceStore: typeof import("../../stores/globalWorkspace").globalWorkspaceStore;

	beforeEach(async () => {
		vi.resetModules();
		createRepositoryRefreshCoordinator = (await import("../../hooks/git/createRepositoryRefreshCoordinator"))
			.createRepositoryRefreshCoordinator;
		repositoriesStore = (await import("../../stores/repositories")).repositoriesStore;
		terminalsStore = (await import("../../stores/terminals")).terminalsStore;
		globalWorkspaceStore = (await import("../../stores/globalWorkspace")).globalWorkspaceStore;
		repositoriesStore._testSetHydrated(true);
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
	});

	const makeCoordinator = (
		worktreePaths: Record<
			string,
			{
				path: string;
				branch: string;
				kind: "worktree";
				warm_artifacts?: import("../../hooks/git/warmStateSeed").WarmArtifactsStatus;
			}
		>,
	) =>
		createRepositoryRefreshCoordinator({
			repo: {
				getInfo: async () => ({ branch: "main", is_git_repo: true }),
				getRepoStructure: async () => ({
					worktree_paths: worktreePaths,
					merged_branches: [],
					in_progress_ops: [],
				}),
				getRepoDiffStats: async () => ({
					diff_stats: {},
					last_commit_ts: {},
					workspace_statuses: {},
				}),
				detectOrphanWorktrees: async () => [],
				assessOrphanCleanup: async () => [],
				beginOrphanCleanup: vi.fn().mockResolvedValue(undefined),
				pendingOrphanCleanupAnswer: async () => null,
				clearOrphanCleanup: vi.fn().mockResolvedValue(undefined),
				removeOrphanWorktree: vi.fn(),
				getWorkspaceLifecycle: vi.fn(),
				finalizeMergedWorktree: vi.fn(),
			},
			dialogs: {},
			closeTerminal: vi.fn(),
			closeTerminalsInWorktree: vi.fn(),
			setStatusInfo: () => {},
		});

	// A session can be `session-created` for a brand-new worktree before the
	// frontend has registered that worktree as a workspace — `assignSessionToRepoBranch`
	// (useAppInit.ts) then has no owner to place it under and parks it in the Global
	// Workspace instead. Nothing used to re-run ownership resolution once the
	// worktree's registration landed, so the terminal stayed parked/invisible until
	// the user happened to select that branch by hand.
	it("re-homes a terminal parked in the Global Workspace once its worktree is newly registered", async () => {
		await testInScopeAsync(async () => {
			repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
			repositoriesStore.setWorkspace("/Gits/alpha", "main", { worktreePath: "/Gits/alpha" });
			repositoriesStore.setActiveWorkspace("/Gits/alpha", "main");

			// Simulate assignSessionToRepoBranch's parking path: no registered
			// workspace owns this cwd yet, so the terminal has repoPath: null and is
			// promoted into the Global Workspace instead of a branch's `terminals`.
			const id = terminalsStore.add({
				sessionId: null,
				fontSize: 14,
				name: "agent",
				cwd: "/Gits/alpha__wt/feature",
				awaitingInput: null,
			});
			terminalsStore.setRepoPath(id, null);
			globalWorkspaceStore.promote(id);

			expect(globalWorkspaceStore.getPromotedIds()).toContain(id);
			expect(repositoriesStore.findOwnerForTerminal(id)).toBeNull();

			const { refreshAllBranchStats } = makeCoordinator({
				feature: { path: "/Gits/alpha__wt/feature", branch: "feature", kind: "worktree" as const },
			});
			await refreshAllBranchStats("/Gits/alpha");

			// The worktree is now registered, and the parked terminal was walked home.
			expect(repositoriesStore.get("/Gits/alpha")?.workspaces.feature?.worktreePath).toBe("/Gits/alpha__wt/feature");
			expect(repositoriesStore.findOwnerForTerminal(id)).toEqual({ repoPath: "/Gits/alpha", workspaceId: "feature" });
			expect(terminalsStore.get(id)?.repoPath).toBe("/Gits/alpha");
			expect(globalWorkspaceStore.getPromotedIds()).not.toContain(id);
		});
	});

	// Baseline regression guard: a refresh that only updates an existing
	// workspace's stats (no newly-registered workspace) must not disturb an
	// already-correctly-placed terminal.
	it("leaves an existing workspace and its terminal untouched when nothing new is registered", async () => {
		await testInScopeAsync(async () => {
			repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
			repositoriesStore.setWorkspace("/Gits/beta", "main", { worktreePath: "/Gits/beta" });
			repositoriesStore.setActiveWorkspace("/Gits/beta", "main");

			const id = terminalsStore.add({
				sessionId: null,
				fontSize: 14,
				name: "shell",
				cwd: "/Gits/beta",
				awaitingInput: null,
			});
			terminalsStore.setRepoPath(id, "/Gits/beta");
			repositoriesStore.addTerminalToWorkspace("/Gits/beta", "main", id);

			const { refreshAllBranchStats } = makeCoordinator({
				main: { path: "/Gits/beta", branch: "main", kind: "worktree" as const },
			});
			await refreshAllBranchStats("/Gits/beta");

			expect(repositoriesStore.get("/Gits/beta")?.workspaces.main?.terminals).toEqual([id]);
			expect(terminalsStore.get(id)?.repoPath).toBe("/Gits/beta");
		});
	});

	// Dropped-items-review #2: after a reload mid-warm the `worktree-warm-*`
	// events are not replayed, so the badge came back only on the next event.
	// The worktree list already carries the backend's warm status; a refresh
	// seeds the badge from it until an event takes over.
	describe("warm badge from the worktree list", () => {
		const warming = { status: "pending", phase: "warming", copied: 3, total: 10 };
		const setup = () => {
			repositoriesStore.add({ path: "/Gits/gamma", displayName: "gamma" });
			repositoriesStore.setWorkspace("/Gits/gamma", "main", { worktreePath: "/Gits/gamma" });
		};

		it("seeds the badge for a worktree whose warm copy is still running", async () => {
			await testInScopeAsync(async () => {
				setup();
				const { refreshAllBranchStats } = makeCoordinator({
					feat: { path: "/Gits/gamma__wt/feat", branch: "feat", kind: "worktree", warm_artifacts: warming },
				});
				await refreshAllBranchStats("/Gits/gamma");

				expect(repositoriesStore.get("/Gits/gamma")?.workspaces.feat?.warmState).toEqual({
					status: "warming",
					copied: 3,
					total: 10,
				});
			});
		});

		it("clears a stale badge once the list says the warm is over, including the later setup phase", async () => {
			await testInScopeAsync(async () => {
				setup();
				const stale = { status: "warming" as const, copied: 1, total: 4 };
				repositoriesStore.setWorkspace("/Gits/gamma", "done", {
					worktreePath: "/Gits/gamma__wt/done",
					warmState: stale,
				});
				repositoriesStore.setWorkspace("/Gits/gamma", "sync", {
					worktreePath: "/Gits/gamma__wt/sync",
					warmState: stale,
				});
				const { refreshAllBranchStats } = makeCoordinator({
					done: { path: "/Gits/gamma__wt/done", branch: "done", kind: "worktree", warm_artifacts: { status: "done" } },
					sync: {
						path: "/Gits/gamma__wt/sync",
						branch: "sync",
						kind: "worktree",
						warm_artifacts: { status: "pending", phase: "file_sync_and_setup_script" },
					},
				});
				await refreshAllBranchStats("/Gits/gamma");

				const workspaces = repositoriesStore.get("/Gits/gamma")?.workspaces;
				expect(workspaces?.done?.warmState).toBeNull();
				expect(workspaces?.sync?.warmState).toBeNull();
			});
		});

		it("leaves the badge to the events once one has arrived for that worktree", async () => {
			await testInScopeAsync(async () => {
				setup();
				const { noteWarmEvent } = await import("../../hooks/git/warmStateSeed");
				const live = { status: "warming" as const, copied: 9, total: 10 };
				repositoriesStore.setWorkspace("/Gits/gamma", "feat", {
					worktreePath: "/Gits/gamma__wt/feat",
					warmState: live,
				});
				noteWarmEvent("/Gits/gamma__wt/feat/");
				// A cached list snapshot older than the event must not move it backwards.
				const { refreshAllBranchStats } = makeCoordinator({
					feat: { path: "/Gits/gamma__wt/feat", branch: "feat", kind: "worktree", warm_artifacts: warming },
				});
				await refreshAllBranchStats("/Gits/gamma");

				expect(repositoriesStore.get("/Gits/gamma")?.workspaces.feat?.warmState).toEqual(live);
			});
		});
	});
});
