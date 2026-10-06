import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { listen } from "@tauri-apps/api/event";
import { createBranchSelectionCoordinator } from "../../hooks/git/createBranchSelectionCoordinator";
import { useWorktreeSwitchPrompt } from "../../hooks/useWorktreeSwitchPrompt";
import { activityStore } from "../../stores/activityStore";
import { repositoriesStore } from "../../stores/repositories";
import { reconcileTerminalOwnership } from "../../stores/terminalOwnership";
import { terminalsStore } from "../../stores/terminals";
import { makeTerminal, testInScopeAsync } from "../helpers/store";
import { mockInvoke } from "../mocks/tauri";

const REPO = "/Gits/critic-placement";
const WORKTREE = "/Gits/critic-placement__wt/feature";

describe("worktree placement lifecycle regressions", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
		repositoriesStore._testSetHydrated(true);
		repositoriesStore.add({ path: REPO, displayName: "critic-placement" });
		repositoriesStore.setWorkspace(REPO, "main", { worktreePath: REPO });
		repositoriesStore.setWorkspace(REPO, "feature", { worktreePath: WORKTREE });
		repositoriesStore.setActiveWorkspace(REPO, "main");
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
		activityStore._testCancelPendingSave();
		terminalsStore._testCancelPendingTimers();
	});

	// Catches: a repository refresh reconciles the creator back to its unchanged shell cwd.
	it("keeps the worktree creator in its new workspace after ownership reconciliation", async () => {
		await testInScopeAsync(async () => {
			const id = terminalsStore.add(makeTerminal({ sessionId: "creator-pty", cwd: REPO, agentType: "codex" }));
			terminalsStore.setRepoPath(id, REPO);
			repositoriesStore.addTerminalToWorkspace(REPO, "main", id);
			const handlers = new Map<string, (event: { payload: unknown }) => void>();
			vi.mocked(listen).mockImplementation(async (name, handler) => {
				handlers.set(name, handler as (event: { payload: unknown }) => void);
				return () => {};
			});
			useWorktreeSwitchPrompt({ handleBranchSelect: async () => {}, closeTerminalsForBranch: async () => {} });
			handlers.get("worktree-created")?.({
				payload: {
					repo_path: REPO,
					workspace_id: "feature",
					branch: "feature",
					worktree_path: WORKTREE,
					kind: "worktree",
					creator_session: "creator-pty",
					spawn_session: false,
				},
			});
			expect(repositoriesStore.findOwnerForTerminal(id)?.workspaceId).toBe("feature");
			reconcileTerminalOwnership();
			expect(repositoriesStore.findOwnerForTerminal(id)?.workspaceId).toBe("feature");
			expect(terminalsStore.get(id)?.cwd).toBe(REPO);
		});
	});

	// Catches: cold restart restores the snapshot but loses the durable declaration on reconciliation.
	it("keeps a declared worktree after restoring a saved agent whose shell cwd stayed in main", async () => {
		await testInScopeAsync(async () => {
			const repo = repositoriesStore.get(REPO);
			if (!repo) throw new Error("fixture repository missing");
			repositoriesStore.setWorkspace(REPO, "feature", {
				savedTerminals: [
					{
						name: "Codex",
						cwd: REPO,
						fontSize: 14,
						agentType: "codex",
						agentSessionId: null,
						tuicSession: "stable-peer",
						agentLaunchCommand: null,
						suspended: true,
					},
				],
			});
			const persisted = JSON.parse(
				JSON.stringify({
					repos: {
						[REPO]: {
							...repo,
							declaredWorktrees: {
								"stable-peer": { workspaceId: "feature", branch: "feature", worktreePath: WORKTREE },
							},
						},
					},
					repoOrder: [REPO],
					activeRepoPath: REPO,
				}),
			);
			mockInvoke.mockImplementation(async (command) => (command === "load_repositories" ? persisted : undefined));
			await repositoriesStore.hydrate();
			const coordinator = createBranchSelectionCoordinator({
				repo: { getDiffStats: async () => ({ additions: 0, deletions: 0 }) },
				pty: { canSpawn: async () => true },
				setStatusInfo: () => {},
				getDefaultFontSize: () => 14,
			});
			await coordinator.handleBranchSelectInner(REPO, "feature");
			// Let both scheduled focus frames finish before disposing the restored scope.
			await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
			const id = terminalsStore.getIds().find((id) => terminalsStore.get(id)?.tuicSession === "stable-peer");
			if (!id) throw new Error("saved agent was not restored");
			expect(repositoriesStore.findOwnerForTerminal(id)?.workspaceId).toBe("feature");
			reconcileTerminalOwnership();
			expect(repositoriesStore.findOwnerForTerminal(id)?.workspaceId).toBe("feature");
			expect(terminalsStore.get(id)?.cwd).toBe(REPO);
		});
	});
});
