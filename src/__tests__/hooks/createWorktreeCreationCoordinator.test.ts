import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

/** Flush pending microtasks (promise chains with no real timers involved). */
async function flushMicrotasks(times = 10) {
	for (let i = 0; i < times; i++) {
		await Promise.resolve();
	}
}

describe("createWorktreeCreationCoordinator", () => {
	let createWorktreeCreationCoordinator: typeof import("../../hooks/git/createWorktreeCreationCoordinator").createWorktreeCreationCoordinator;
	let repositoriesStore: typeof import("../../stores/repositories").repositoriesStore;
	let repoSettingsStore: typeof import("../../stores/repoSettings").repoSettingsStore;
	let mockListen: ReturnType<typeof vi.fn>;

	const REPO = "/Gits/alpha";

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		// `../../invoke`'s `listen()` routes through `@tauri-apps/api/event`'s `listen`
		// whenever `isTauri()` is true (global setup sets `__TAURI_INTERNALS__`). Mock
		// it here — rather than relying on the global `../mocks/tauri.ts` registration —
		// so `captureListener` below always sees the exact instance the freshly
		// re-imported (post-`resetModules`) coordinator module resolves to.
		mockListen = vi.fn().mockResolvedValue(vi.fn());
		vi.doMock("@tauri-apps/api/event", () => ({ listen: mockListen, emit: vi.fn().mockResolvedValue(undefined) }));
		createWorktreeCreationCoordinator = (await import("../../hooks/git/createWorktreeCreationCoordinator"))
			.createWorktreeCreationCoordinator;
		repositoriesStore = (await import("../../stores/repositories")).repositoriesStore;
		repoSettingsStore = (await import("../../stores/repoSettings")).repoSettingsStore;
		repositoriesStore._testSetHydrated(true);
		repositoriesStore.add({ path: REPO, displayName: "alpha" });
	});

	/** Capture the handler registered for `eventName` via `listen()`, whatever else
	 *  is also registered. Mirrors `useAppInit.test.ts`'s `captureListener`. */
	function captureListener<T>(eventName: string) {
		let callback: ((event: { payload: T }) => void) | null = null;
		mockListen.mockImplementation((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === eventName) {
				callback = handler as unknown as (event: { payload: T }) => void;
			}
			return Promise.resolve(vi.fn());
		});
		return { getCallback: () => callback };
	}

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
	});

	/** Builds a coordinator with real solid signals for worktreeDialogState/creatingWorktreeRepos
	 * (so effects reading them behave normally) and mock-everything-else deps. */
	function makeCoordinator(depOverrides: Record<string, unknown> = {}) {
		const [creatingWorktreeRepos, setCreatingWorktreeRepos] =
			(depOverrides.creatingWorktreeRepos as
				| [ReturnType<typeof createSignal<Set<string>>>[0], ReturnType<typeof createSignal<Set<string>>>[1]]
				| undefined) ?? createSignal<Set<string>>(new Set());
		const [worktreeDialogState, setWorktreeDialogState] = createSignal<
			import("../../hooks/git/createWorktreeCreationCoordinator").WorktreeDialogState | null
		>(null);
		const statusMessages: string[] = [];
		const markRecentlyCreated = vi.fn();
		const handleAddTerminalToWorkspace = vi.fn().mockResolvedValue(undefined);

		const repo = {
			generateWorktreeName: vi.fn().mockResolvedValue("new-worktree"),
			generateCloneBranchName: vi.fn().mockResolvedValue("clone-name"),
			listLocalBranches: vi.fn().mockResolvedValue([]),
			listBaseRefOptions: vi.fn().mockResolvedValue([
				{ name: "main", kind: "local", is_default: true },
				{ name: "develop", kind: "local", is_default: false },
			]),
			createWorktree: vi.fn().mockResolvedValue({
				status: "ok",
				name: "new-worktree",
				path: `${REPO}__wt/new-worktree`,
				workspace_id: "new-worktree",
				branch: "new-worktree",
				base_repo: REPO,
			}),
			getDiffStats: vi.fn().mockResolvedValue({ additions: 0, deletions: 0 }),
			...(depOverrides.repo as Record<string, unknown> | undefined),
		};

		const pty = {
			getWorktreesDir: vi.fn().mockResolvedValue(`${REPO}__wt`),
			...(depOverrides.pty as Record<string, unknown> | undefined),
		};

		const coordinator = createWorktreeCreationCoordinator({
			...depOverrides,
			repo,
			pty,
			setStatusInfo: (m: string) => statusMessages.push(m),
			creatingWorktreeRepos,
			setCreatingWorktreeRepos,
			worktreeDialogState,
			setWorktreeDialogState,
			markRecentlyCreated,
			handleAddTerminalToWorkspace,
		} as never);

		return {
			coordinator,
			statusMessages,
			repo,
			pty,
			creatingWorktreeRepos,
			setCreatingWorktreeRepos,
			worktreeDialogState,
			setWorktreeDialogState,
			markRecentlyCreated,
			handleAddTerminalToWorkspace,
		};
	}

	describe("handleAddWorktree", () => {
		it("fetches dialog data in parallel and opens the dialog with baseRefs[0] as the default when nothing is remembered", async () => {
			await testInScopeAsync(async () => {
				const { coordinator, worktreeDialogState } = makeCoordinator();

				await coordinator.handleAddWorktree(REPO);

				const state = worktreeDialogState();
				expect(state).toMatchObject({
					repoPath: REPO,
					suggestedName: "new-worktree",
					worktreesDir: `${REPO}__wt`,
					defaultBaseRef: "main",
				});
				expect(state?.baseRefs).toEqual([
					{ name: "main", kind: "local", is_default: true },
					{ name: "develop", kind: "local", is_default: false },
				]);
			});
		});

		it("does nothing when a creation is already in flight for this repo", async () => {
			await testInScopeAsync(async () => {
				const creatingSignal = createSignal<Set<string>>(new Set([REPO]));
				const { coordinator, worktreeDialogState, repo } = makeCoordinator({ creatingWorktreeRepos: creatingSignal });

				await coordinator.handleAddWorktree(REPO);

				expect(repo.listBaseRefOptions).not.toHaveBeenCalled();
				expect(worktreeDialogState()).toBeNull();
			});
		});

		it("skips the dialog and creates instantly when promptOnCreate is false, using defaultBaseRef", async () => {
			await testInScopeAsync(async () => {
				const getPromptOnCreate = vi.fn().mockReturnValue(false);
				const { coordinator, repo, worktreeDialogState } = makeCoordinator({ getPromptOnCreate });

				await coordinator.handleAddWorktree(REPO);

				expect(repo.createWorktree).toHaveBeenCalledWith(REPO, "new-worktree", true, "main");
				// The dialog state was set (so confirmCreateWorktree could read repoPath from it)
				// but is cleared again once creation completes successfully.
				expect(worktreeDialogState()).toBeNull();
			});
		});

		it("falls back to 'HEAD' when promptOnCreate is false and there is no base ref at all", async () => {
			await testInScopeAsync(async () => {
				const getPromptOnCreate = vi.fn().mockReturnValue(false);
				const repoOverrides = { listBaseRefOptions: vi.fn().mockResolvedValue([]) };
				const { coordinator, repo } = makeCoordinator({ getPromptOnCreate, repo: repoOverrides });

				await coordinator.handleAddWorktree(REPO);

				expect(repo.createWorktree).toHaveBeenCalledWith(REPO, "new-worktree", true, "HEAD");
			});
		});

		describe("resolveDefaultBaseRef via the repo's configured 'Branch From' setting", () => {
			it("uses the configured branch as the default when it exists among baseRefs", async () => {
				await testInScopeAsync(async () => {
					repoSettingsStore.getOrCreate(REPO, "alpha");
					repoSettingsStore.update(REPO, { baseBranch: "develop" });
					const { coordinator, worktreeDialogState } = makeCoordinator();

					await coordinator.handleAddWorktree(REPO);

					const state = worktreeDialogState();
					expect(state?.defaultBaseRef).toBe("develop");
					expect(state?.missingBaseBranch).toBeUndefined();
				});
			});

			it("ignores the 'automatic' setting value and falls back to baseRefs[0]", async () => {
				await testInScopeAsync(async () => {
					repoSettingsStore.getOrCreate(REPO, "alpha");
					repoSettingsStore.update(REPO, { baseBranch: "automatic" });
					const { coordinator, worktreeDialogState } = makeCoordinator();

					await coordinator.handleAddWorktree(REPO);

					const state = worktreeDialogState();
					expect(state?.defaultBaseRef).toBe("main");
					expect(state?.missingBaseBranch).toBeUndefined();
				});
			});

			it("falls back to baseRefs[0] and flags missingBaseBranch when the configured branch no longer exists", async () => {
				await testInScopeAsync(async () => {
					repoSettingsStore.getOrCreate(REPO, "alpha");
					repoSettingsStore.update(REPO, { baseBranch: "long-gone" });
					const { coordinator, worktreeDialogState } = makeCoordinator();

					await coordinator.handleAddWorktree(REPO);

					const state = worktreeDialogState();
					expect(state?.defaultBaseRef).toBe("main");
					expect(state?.missingBaseBranch).toBe("long-gone");
				});
			});

			it("prefers the session-remembered ref over the configured setting", async () => {
				await testInScopeAsync(async () => {
					repoSettingsStore.getOrCreate(REPO, "alpha");
					repoSettingsStore.update(REPO, { baseBranch: "develop" });
					const { coordinator, worktreeDialogState } = makeCoordinator();

					await coordinator.handleAddWorktree(REPO);
					await coordinator.confirmCreateWorktree({ branchName: "feature-x", createBranch: true, baseRef: "main" });

					await coordinator.handleAddWorktree(REPO);
					const state = worktreeDialogState();
					expect(state?.defaultBaseRef).toBe("main");
					expect(state?.missingBaseBranch).toBeUndefined();
				});
			});

			it("surfaces missingBaseBranch via setStatusInfo and still creates when promptOnCreate is false", async () => {
				await testInScopeAsync(async () => {
					repoSettingsStore.getOrCreate(REPO, "alpha");
					repoSettingsStore.update(REPO, { baseBranch: "long-gone" });
					const getPromptOnCreate = vi.fn().mockReturnValue(false);
					const { coordinator, repo, statusMessages } = makeCoordinator({ getPromptOnCreate });

					await coordinator.handleAddWorktree(REPO);

					expect(repo.createWorktree).toHaveBeenCalledWith(REPO, "new-worktree", true, "main");
					expect(statusMessages.some((m) => m.includes("long-gone"))).toBe(true);
				});
			});
		});
	});

	describe("confirmCreateWorktree", () => {
		it("does nothing when there is no open dialog state", async () => {
			await testInScopeAsync(async () => {
				const { coordinator, repo } = makeCoordinator();
				await coordinator.confirmCreateWorktree({ branchName: "x", createBranch: true, baseRef: "main" });
				expect(repo.createWorktree).not.toHaveBeenCalled();
			});
		});

		it("remembers the base ref used on success, and that remembered ref wins over baseRefs[0] on the next open", async () => {
			await testInScopeAsync(async () => {
				const { coordinator, worktreeDialogState } = makeCoordinator();

				await coordinator.handleAddWorktree(REPO);
				expect(worktreeDialogState()?.defaultBaseRef).toBe("main");

				await coordinator.confirmCreateWorktree({ branchName: "feature-x", createBranch: true, baseRef: "develop" });

				await coordinator.handleAddWorktree(REPO);
				expect(worktreeDialogState()?.defaultBaseRef).toBe("develop");
			});
		});

		it("discards a remembered ref that is no longer in baseRefs, falling back to baseRefs[0]", async () => {
			await testInScopeAsync(async () => {
				const { coordinator, worktreeDialogState, repo } = makeCoordinator();

				await coordinator.handleAddWorktree(REPO);
				await coordinator.confirmCreateWorktree({ branchName: "feature-x", createBranch: true, baseRef: "develop" });

				// Next open: "develop" has since been deleted from baseRefs.
				repo.listBaseRefOptions.mockResolvedValue([{ name: "main", kind: "local", is_default: true }]);
				await coordinator.handleAddWorktree(REPO);
				expect(worktreeDialogState()?.defaultBaseRef).toBe("main");
			});
		});

		it("does not remember a base ref when creation throws", async () => {
			await testInScopeAsync(async () => {
				const repoOverrides = { createWorktree: vi.fn().mockRejectedValue(new Error("boom")) };
				const { coordinator, worktreeDialogState } = makeCoordinator({ repo: repoOverrides });

				await coordinator.handleAddWorktree(REPO);
				await expect(
					coordinator.confirmCreateWorktree({ branchName: "feature-x", createBranch: true, baseRef: "develop" }),
				).rejects.toThrow("boom");

				// Dialog stays open (not cleared) after a failed creation.
				expect(worktreeDialogState()).not.toBeNull();

				// Re-resolve with a fresh coordinator sharing nothing — "develop" must not
				// have been remembered by the failed attempt above.
				const second = makeCoordinator();
				await second.coordinator.handleAddWorktree(REPO);
				expect(second.worktreeDialogState()?.defaultBaseRef).toBe("main");
			});
		});

		it("short-circuits when a creation is already in flight for this repo", async () => {
			await testInScopeAsync(async () => {
				const creatingSignal = createSignal<Set<string>>(new Set([REPO]));
				const { coordinator, repo, worktreeDialogState, setWorktreeDialogState } = makeCoordinator({
					creatingWorktreeRepos: creatingSignal,
				});
				setWorktreeDialogState({
					repoPath: REPO,
					suggestedName: "x",
					existingBranches: [],
					worktreeBranches: [],
					worktreesDir: `${REPO}__wt`,
					baseRefs: [],
					defaultBaseRef: "",
				});

				await coordinator.confirmCreateWorktree({ branchName: "x", createBranch: true, baseRef: "main" });

				expect(repo.createWorktree).not.toHaveBeenCalled();
				expect(worktreeDialogState()).not.toBeNull();
			});
		});
	});

	describe("setupNewWorktree — setup-script/run-script ordering", () => {
		/** `makeCoordinator`'s default `createWorktree` mock always resolves with a fixed
		 *  `branch: "new-worktree"`, regardless of the requested branch name — fine for
		 *  the other describe blocks, but `waitForSetupScriptCompletion` matches the
		 *  event's `branch` against `result.branch`, so these tests need the mock to
		 *  actually echo back the requested branch name. */
		const echoBranchRepoOverrides = {
			createWorktree: vi.fn().mockImplementation((baseRepo: string, branchName: string) =>
				Promise.resolve({
					status: "ok",
					name: branchName,
					path: `${REPO}__wt/${branchName}`,
					workspace_id: branchName,
					branch: branchName,
					base_repo: baseRepo,
				}),
			),
		};

		it("waits for the matching worktree-setup-script-completed event before creating the terminal, when a setup script is configured", async () => {
			await testInScopeAsync(async () => {
				repoSettingsStore.getOrCreate(REPO, "alpha");
				repoSettingsStore.update(REPO, { setupScript: "npm install" });
				const { getCallback } = captureListener<{ repoPath: string; branch: string }>(
					"worktree-setup-script-completed",
				);
				const { coordinator, handleAddTerminalToWorkspace } = makeCoordinator({ repo: echoBranchRepoOverrides });

				await coordinator.handleAddWorktree(REPO);
				const done = coordinator.confirmCreateWorktree({
					branchName: "feature-x",
					createBranch: true,
					baseRef: "main",
				});

				await flushMicrotasks();
				expect(mockListen).toHaveBeenCalledWith("worktree-setup-script-completed", expect.any(Function));
				expect(handleAddTerminalToWorkspace).not.toHaveBeenCalled();

				getCallback()!({ payload: { repoPath: REPO, branch: "feature-x" } });
				await done;

				expect(handleAddTerminalToWorkspace).toHaveBeenCalledWith(REPO, "feature-x");
			});
		});

		it("keeps waiting on a non-matching event (different repo or branch)", async () => {
			await testInScopeAsync(async () => {
				repoSettingsStore.getOrCreate(REPO, "alpha");
				repoSettingsStore.update(REPO, { setupScript: "npm install" });
				const { getCallback } = captureListener<{ repoPath: string; branch: string }>(
					"worktree-setup-script-completed",
				);
				const { coordinator, handleAddTerminalToWorkspace } = makeCoordinator({ repo: echoBranchRepoOverrides });

				await coordinator.handleAddWorktree(REPO);
				const done = coordinator.confirmCreateWorktree({
					branchName: "feature-x",
					createBranch: true,
					baseRef: "main",
				});
				await flushMicrotasks();

				getCallback()!({ payload: { repoPath: REPO, branch: "some-other-branch" } });
				await flushMicrotasks();
				expect(handleAddTerminalToWorkspace).not.toHaveBeenCalled();

				getCallback()!({ payload: { repoPath: REPO, branch: "feature-x" } });
				await done;
				expect(handleAddTerminalToWorkspace).toHaveBeenCalledWith(REPO, "feature-x");
			});
		});

		it("creates the terminal without waiting on any event when no setup script is configured", async () => {
			await testInScopeAsync(async () => {
				const { coordinator, handleAddTerminalToWorkspace } = makeCoordinator({ repo: echoBranchRepoOverrides });

				await coordinator.handleAddWorktree(REPO);
				await coordinator.confirmCreateWorktree({ branchName: "feature-x", createBranch: true, baseRef: "main" });

				expect(mockListen).not.toHaveBeenCalled();
				expect(handleAddTerminalToWorkspace).toHaveBeenCalledWith(REPO, "feature-x");
			});
		});

		it("gives up and proceeds anyway once the safety-net timeout elapses with no matching event", async () => {
			vi.useFakeTimers();
			try {
				await testInScopeAsync(async () => {
					repoSettingsStore.getOrCreate(REPO, "alpha");
					repoSettingsStore.update(REPO, { setupScript: "npm install" });
					captureListener("worktree-setup-script-completed");
					const { coordinator, handleAddTerminalToWorkspace } = makeCoordinator({ repo: echoBranchRepoOverrides });

					await coordinator.handleAddWorktree(REPO);
					const done = coordinator.confirmCreateWorktree({
						branchName: "feature-x",
						createBranch: true,
						baseRef: "main",
					});
					await flushMicrotasks();
					expect(handleAddTerminalToWorkspace).not.toHaveBeenCalled();

					// Matches SETUP_SCRIPT_WAIT_TIMEOUT_MS in createWorktreeCreationCoordinator.ts.
					await vi.advanceTimersByTimeAsync(900_000);
					await done;

					expect(handleAddTerminalToWorkspace).toHaveBeenCalledWith(REPO, "feature-x");
				});
			} finally {
				vi.useRealTimers();
			}
		});

		it("still unlistens if listen()'s own registration promise resolves after the safety-net timeout already fired", async () => {
			// Tauri's real listen() is itself async — there's a genuine window where
			// the safety-net timeout can fire before it resolves. If the late
			// registration didn't check `settled` and unlisten immediately, this
			// would leak a live "worktree-setup-script-completed" listener forever.
			vi.useFakeTimers();
			try {
				await testInScopeAsync(async () => {
					repoSettingsStore.getOrCreate(REPO, "alpha");
					repoSettingsStore.update(REPO, { setupScript: "npm install" });

					const unlistenFn = vi.fn();
					let resolveRegistration: (() => void) | undefined;
					mockListen.mockImplementation(
						() =>
							new Promise<() => void>((resolve) => {
								resolveRegistration = () => resolve(unlistenFn);
							}),
					);

					const { coordinator, handleAddTerminalToWorkspace } = makeCoordinator({ repo: echoBranchRepoOverrides });

					await coordinator.handleAddWorktree(REPO);
					const done = coordinator.confirmCreateWorktree({
						branchName: "feature-x",
						createBranch: true,
						baseRef: "main",
					});
					await flushMicrotasks();

					// Timeout fires with listen()'s registration promise still pending.
					await vi.advanceTimersByTimeAsync(900_000);
					await done;
					expect(handleAddTerminalToWorkspace).toHaveBeenCalledWith(REPO, "feature-x");
					expect(unlistenFn).not.toHaveBeenCalled();

					// Now the registration finally resolves — must unlisten immediately
					// rather than leaving a live listener registered indefinitely.
					resolveRegistration?.();
					await flushMicrotasks();
					expect(unlistenFn).toHaveBeenCalledTimes(1);
				});
			} finally {
				vi.useRealTimers();
			}
		});
	});
});
