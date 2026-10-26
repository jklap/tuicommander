import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createTerminalWorktreeCoordinator } from "../../hooks/git/createTerminalWorktreeCoordinator";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { makeTerminal } from "../helpers/store";

const REPO = "/repo";

/**
 * A stale ex-root row can sit next to the real one, both claiming the repo root
 * (seen live: `wip-bkup` beside `wip`). A cwd at the root names no row, so the
 * tracker picks one — it must not pull a terminal off the row it was placed on.
 */
describe("cwd tracking with two rows at the repo root", () => {
	let coordinator: ReturnType<typeof createTerminalWorktreeCoordinator>;

	beforeEach(() => {
		vi.useFakeTimers();
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
		repositoriesStore.add({ path: REPO, displayName: "repo" });
		// Insertion order matters: the stale row comes first, as it does live.
		repositoriesStore.setWorkspace(REPO, "wip-bkup", { worktreePath: REPO, kind: "main" });
		repositoriesStore.setWorkspace(REPO, "wip", { worktreePath: REPO, kind: "main" });
		repositoriesStore.setActiveWorkspace(REPO, "wip-bkup");
		coordinator = createTerminalWorktreeCoordinator({
			refreshBranches: vi.fn().mockResolvedValue(undefined),
			writePty: vi.fn().mockResolvedValue(undefined),
		});
	});

	afterEach(() => {
		vi.useRealTimers();
		repositoriesStore._testCancelPendingSave();
	});

	const addTerminal = (workspaceId: string, cwd: string) => {
		const id = terminalsStore.add(makeTerminal({ name: "t", sessionId: `s-${workspaceId}`, cwd }));
		terminalsStore.setRepoPath(id, REPO);
		repositoriesStore.addTerminalToWorkspace(REPO, workspaceId, id);
		return id;
	};

	const cwdChange = async (id: string, cwd: string) => {
		coordinator.handleTerminalCwdChange(id, cwd);
		await vi.advanceTimersByTimeAsync(300);
		coordinator.cancelCwdTracking(id);
	};

	it("leaves a terminal on its own root row when the cwd is the repo root", async () => {
		const id = addTerminal("wip", REPO);

		await cwdChange(id, REPO);

		expect(repositoriesStore.findOwnerForTerminal(id)?.workspaceId).toBe("wip");
		expect(repositoriesStore.get(REPO)?.workspaces["wip-bkup"].terminals).not.toContain(id);
	});

	it("falls back to the old placement when the terminal's own row records no directory", async () => {
		repositoriesStore.setWorkspace(REPO, "bare"); // no worktreePath recorded
		const id = addTerminal("bare", REPO);

		await cwdChange(id, REPO);

		// The guard needs a recorded directory to compare; without one the tracker
		// resolves the root exactly as before and the terminal moves to a root row.
		expect(["wip", "wip-bkup"]).toContain(repositoriesStore.findOwnerForTerminal(id)?.workspaceId);
	});

	it("never moves a terminal across repos on its own", async () => {
		repositoriesStore.add({ path: "/other", displayName: "other" });
		repositoriesStore.setWorkspace("/other", "main", { worktreePath: "/other" });
		const id = addTerminal("wip", REPO);

		await cwdChange(id, "/other");

		expect(repositoriesStore.findOwnerForTerminal(id)).toEqual({ repoPath: REPO, workspaceId: "wip" });
	});

	it("still re-homes a terminal that cd's into a linked worktree", async () => {
		repositoriesStore.setWorkspace(REPO, "feature", { worktreePath: `${REPO}/.worktrees/feature`, kind: "worktree" });
		const id = addTerminal("wip", REPO);

		await cwdChange(id, `${REPO}/.worktrees/feature`);

		expect(repositoriesStore.findOwnerForTerminal(id)?.workspaceId).toBe("feature");
	});

	it("still settles a terminal on a root row when it came from a linked worktree", async () => {
		repositoriesStore.setWorkspace(REPO, "feature", { worktreePath: `${REPO}/.worktrees/feature`, kind: "worktree" });
		const id = addTerminal("feature", `${REPO}/.worktrees/feature`);

		await cwdChange(id, REPO);

		const home = repositoriesStore.findOwnerForTerminal(id)?.workspaceId;
		expect(["wip", "wip-bkup"]).toContain(home);
	});
});
