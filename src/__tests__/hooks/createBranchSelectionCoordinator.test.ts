import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

describe("createBranchSelectionCoordinator", () => {
	let createBranchSelectionCoordinator: typeof import("../../hooks/git/createBranchSelectionCoordinator").createBranchSelectionCoordinator;
	let repositoriesStore: typeof import("../../stores/repositories").repositoriesStore;
	let terminalsStore: typeof import("../../stores/terminals").terminalsStore;
	let paneLayoutStore: typeof import("../../stores/paneLayout").paneLayoutStore;
	let settingsStore: typeof import("../../stores/settings").settingsStore;

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		createBranchSelectionCoordinator = (await import("../../hooks/git/createBranchSelectionCoordinator"))
			.createBranchSelectionCoordinator;
		repositoriesStore = (await import("../../stores/repositories")).repositoriesStore;
		terminalsStore = (await import("../../stores/terminals")).terminalsStore;
		paneLayoutStore = (await import("../../stores/paneLayout")).paneLayoutStore;
		settingsStore = (await import("../../stores/settings")).settingsStore;
		repositoriesStore._testSetHydrated(true);
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
		paneLayoutStore._testCancelPendingSave();
		settingsStore._testCancelPendingSave();
	});

	const makeCoordinator = () =>
		createBranchSelectionCoordinator({
			repo: { getDiffStats: async () => ({ additions: 0, deletions: 0 }) },
			pty: { canSpawn: async () => true },
			setStatusInfo: () => {},
			getDefaultFontSize: () => 14,
			setCurrentRepoPath: (() => {}) as never,
			setCurrentBranch: (() => {}) as never,
		});

	// `TerminalData.repoPath` is the owning repo of record — the field
	// `reconcileTerminalOwnership` trusts over the branch arrays, where `null`
	// means "no registered repo claims this cwd, the placement is a guess". This
	// path is the opposite of a guess: it is handed the repo. Leaving the field
	// null here filed a deliberately placed terminal as parked.
	it("records the owning repo on a terminal it adds to a branch", async () => {
		await testInScope(async () => {
			repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
			repositoriesStore.setBranch("/Gits/alpha", "main", { worktreePath: "/Gits/alpha" });
			repositoriesStore.setActiveBranch("/Gits/alpha", "main");

			const id = await makeCoordinator().handleAddTerminalToBranch("/Gits/alpha", "main");

			expect(id).toBeTruthy();
			expect(terminalsStore.get(id!)?.repoPath).toBe("/Gits/alpha");
			expect(repositoriesStore.findOwnerForTerminal(id!)).toEqual({
				repoPath: "/Gits/alpha",
				branchName: "main",
			});
		});
	});

	// `crypto.randomUUID` is only defined in a secure context (https, or
	// localhost). TUIC is also reached over plain http on a LAN address, where
	// a bare call throws instead of minting the terminal's `tuicSession` id.
	it("still adds a terminal when crypto.randomUUID is unavailable (non-secure-context remote clients)", async () => {
		const realCrypto = globalThis.crypto;
		vi.stubGlobal("crypto", {});
		try {
			await testInScope(async () => {
				repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
				repositoriesStore.setBranch("/Gits/alpha", "main", { worktreePath: "/Gits/alpha" });
				repositoriesStore.setActiveBranch("/Gits/alpha", "main");

				const id = await makeCoordinator().handleAddTerminalToBranch("/Gits/alpha", "main");

				expect(id).toBeTruthy();
				expect(terminalsStore.get(id!)?.tuicSession).toBeTruthy();
			});
		} finally {
			vi.stubGlobal("crypto", realCrypto);
		}
	});

	it("does not create a terminal when the spawn budget is exhausted", async () => {
		await testInScope(async () => {
			repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
			repositoriesStore.setBranch("/Gits/alpha", "main", { worktreePath: "/Gits/alpha" });

			const messages: string[] = [];
			const coordinator = createBranchSelectionCoordinator({
				repo: { getDiffStats: async () => ({ additions: 0, deletions: 0 }) },
				pty: { canSpawn: async () => false },
				setStatusInfo: (message) => messages.push(message),
				getDefaultFontSize: () => 14,
				setCurrentRepoPath: (() => {}) as never,
				setCurrentBranch: (() => {}) as never,
			});

			expect(await coordinator.handleAddTerminalToBranch("/Gits/alpha", "main")).toBeUndefined();
			expect(terminalsStore.getIds()).toHaveLength(0);
			expect(messages).toEqual(["Max sessions reached (50)"]);
		});
	});

	// --- Characterization tests for savedTerminals restore (createBranchSelectionCoordinator.ts:235-304) ---
	// These pin down TODAY's behavior — agent-only restore — before a later change
	// makes shell restoration configurable. Any intentional behavior change here
	// must update these assertions, not just make them pass.
	describe("savedTerminals restore", () => {
		// handleBranchSelectInner schedules two nested requestAnimationFrame calls
		// to focus the active terminal. Let those real timers fire before the test
		// ends, or vitest's async-leak detector flags them as leaked.
		const flushRaf = () => new Promise<void>((resolve) => setTimeout(resolve, 50));

		const shellSaved = { name: "shell", cwd: "/Gits/alpha", fontSize: 14, agentType: null };
		const agentSaved = {
			name: "claude",
			cwd: "/Gits/alpha",
			fontSize: 14,
			agentType: "claude" as const,
			agentSessionId: "sess-1",
			tuicSession: "tuic-1",
			agentLaunchCommand: null,
		};

		function setupBranch(savedTerminals: unknown[]) {
			repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
			repositoriesStore.setBranch("/Gits/alpha", "main", {
				worktreePath: "/Gits/alpha",
				hadTerminals: true,
				// biome-ignore lint/suspicious/noExplicitAny: test fixture, shape matches SavedTerminal
				savedTerminals: savedTerminals as any,
			});
		}

		it("restores an agent tab and clears savedTerminals (consume-once)", async () => {
			await testInScope(async () => {
				setupBranch([agentSaved]);

				await makeCoordinator().handleBranchSelect("/Gits/alpha", "main");
				await flushRaf();

				const ids = terminalsStore.getIds();
				expect(ids).toHaveLength(1);
				const term = terminalsStore.get(ids[0]);
				expect(term?.cwd).toBe("/Gits/alpha");
				expect(term?.agentType).toBe("claude");
				expect(term?.agentSessionId).toBe("sess-1");
				expect(term?.tuicSession).toBe("tuic-1");
				expect(repositoriesStore.get("/Gits/alpha")?.branches.main?.savedTerminals).toEqual([]);
			});
		});

		it("restores a shell tab as a fresh live shell in its saved cwd (restoreShellTerminals default on)", async () => {
			await testInScope(async () => {
				setupBranch([shellSaved]);

				await makeCoordinator().handleBranchSelect("/Gits/alpha", "main");
				await flushRaf();

				const ids = terminalsStore.getIds();
				expect(ids).toHaveLength(1);
				const term = terminalsStore.get(ids[0]);
				expect(term?.cwd).toBe("/Gits/alpha");
				expect(term?.name).toBe("shell");
				expect(term?.agentType == null).toBe(true);
				expect(repositoriesStore.get("/Gits/alpha")?.branches.main?.savedTerminals).toEqual([]);
			});
		});

		it("restores both the shell and the agent tab from a mixed list (restoreShellTerminals default on)", async () => {
			await testInScope(async () => {
				setupBranch([shellSaved, agentSaved]);

				await makeCoordinator().handleBranchSelect("/Gits/alpha", "main");
				await flushRaf();

				const ids = terminalsStore.getIds();
				expect(ids).toHaveLength(2);
				const agentTypes = ids.map((id) => terminalsStore.get(id)?.agentType);
				expect(agentTypes).toContain("claude");
				expect(agentTypes).toContain(null);
			});
		});

		it("discards shell-only savedTerminals and spawns one fresh terminal when restoreShellTerminals is off", async () => {
			await testInScope(async () => {
				settingsStore.setRestoreShellTerminals(false);
				setupBranch([shellSaved]);

				await makeCoordinator().handleBranchSelect("/Gits/alpha", "main");
				await flushRaf();

				const ids = terminalsStore.getIds();
				expect(ids).toHaveLength(1);
				const term = terminalsStore.get(ids[0]);
				// The fresh spawn (handleAddTerminalToBranch), not a restored shell:
				// it gets a fresh sessionId=null tab with no agentType, not the saved name.
				expect(term?.agentType == null).toBe(true);
				expect(term?.name).not.toBe("shell");
				expect(repositoriesStore.get("/Gits/alpha")?.branches.main?.savedTerminals).toEqual([]);
			});
		});

		it("restores only the agent tab from a mixed shell+agent savedTerminals list when restoreShellTerminals is off", async () => {
			await testInScope(async () => {
				settingsStore.setRestoreShellTerminals(false);
				setupBranch([shellSaved, agentSaved]);

				await makeCoordinator().handleBranchSelect("/Gits/alpha", "main");
				await flushRaf();

				const ids = terminalsStore.getIds();
				expect(ids).toHaveLength(1);
				expect(terminalsStore.get(ids[0])?.agentType).toBe("claude");
			});
		});

		it("leaves savedTerminals untouched when live valid terminals already exist", async () => {
			await testInScope(async () => {
				repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
				const id = terminalsStore.add({
					sessionId: "s1",
					fontSize: 14,
					name: "live",
					cwd: "/Gits/alpha",
					awaitingInput: null,
				});
				repositoriesStore.setBranch("/Gits/alpha", "main", {
					worktreePath: "/Gits/alpha",
					hadTerminals: true,
					terminals: [id],
					// biome-ignore lint/suspicious/noExplicitAny: test fixture
					savedTerminals: [agentSaved] as any,
				});

				await makeCoordinator().handleBranchSelect("/Gits/alpha", "main");
				await flushRaf();

				expect(terminalsStore.getIds()).toEqual([id]);
				expect(repositoriesStore.get("/Gits/alpha")?.branches.main?.savedTerminals).toEqual([agentSaved]);
			});
		});

		it("auto-spawns a terminal the first time a branch is selected (hadTerminals=false)", async () => {
			await testInScope(async () => {
				repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
				repositoriesStore.setBranch("/Gits/alpha", "main", {
					worktreePath: "/Gits/alpha",
					hadTerminals: false,
				});

				await makeCoordinator().handleBranchSelect("/Gits/alpha", "main");
				await flushRaf();

				expect(terminalsStore.getIds()).toHaveLength(1);
			});
		});

		it("shows empty state when hadTerminals=true and no valid or saved terminals remain", async () => {
			await testInScope(async () => {
				repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
				repositoriesStore.setBranch("/Gits/alpha", "main", {
					worktreePath: "/Gits/alpha",
					hadTerminals: true,
					terminals: ["stale-gone-id"],
				});

				await makeCoordinator().handleBranchSelect("/Gits/alpha", "main");
				await flushRaf();

				expect(terminalsStore.getIds()).toHaveLength(0);
				expect(terminalsStore.state.activeId).toBeNull();
			});
		});

		it("remaps a disk-restored pane layout's terminal ids onto the newly restored terminals", async () => {
			await testInScope(async () => {
				mockInvoke.mockImplementation(async (cmd: string) => {
					if (cmd === "load_pane_layout") {
						return {
							root: { type: "leaf", id: "g1" },
							groups: {
								g1: { id: "g1", tabs: [{ id: "old-term-id", type: "terminal" }], activeTabId: "old-term-id" },
							},
							activeGroupId: "g1",
						};
					}
					return undefined;
				});
				await paneLayoutStore.loadFromDisk();

				setupBranch([agentSaved]);

				await makeCoordinator().handleBranchSelect("/Gits/alpha", "main");
				await flushRaf();

				const [newId] = terminalsStore.getIds();
				const remapped = paneLayoutStore.getTerminalTabIds();
				expect(remapped).toEqual([newId]);
			});
		});
	});
});
