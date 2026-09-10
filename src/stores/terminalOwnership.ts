import { batch } from "solid-js";
import { appLogger } from "./appLogger";
import { globalWorkspaceStore } from "./globalWorkspace";
import { placementBranchFor, repositoriesStore, resolveRepoOwner } from "./repositories";
import { terminalsStore } from "./terminals";

/**
 * Re-home terminals whose branch placement disagrees with their own cwd.
 *
 * A terminal whose cwd matched no registered repo is parked in the Global
 * Workspace, with `repoPath: null` recording that it has no owner yet. Nothing
 * used to undo that: registering the real repo afterwards left the tab stranded,
 * which is one half of the tabs showing up in the wrong place. (Until 2026-09-10
 * the parking spot was the ACTIVE repo, which was worse than stranded — two
 * sessions from one unregistered repo landed under two different repos depending
 * on where the user was standing.) The other half is a placement that was correct
 * once and went stale — a repo removed, a worktree added, a branch renamed.
 *
 * Both are the same question asked again: given this cwd and the repos we know
 * about NOW, who owns this terminal? Call it whenever that answer can change —
 * after repositories load, after one is added or removed, and after a terminal
 * reports a new cwd.
 *
 * Pass `terminalId` when only one terminal's answer can have changed. A `cd`
 * fires OSC 7 on every directory change, and walking every terminal against
 * every repo on each of those is work nobody asked for.
 */
export function reconcileTerminalOwnership(terminalId?: string): void {
	let moved = 0;

	const scope = terminalId === undefined ? terminalsStore.getIds() : [terminalId];
	for (const terminalId of scope) {
		const terminal = terminalsStore.get(terminalId);
		if (!terminal) continue;

		const owner = resolveRepoOwner(terminal.cwd);
		// Still unclaimed. Leave the parked tab where it is — moving it nowhere
		// would only make it invisible.
		if (!owner) continue;

		const branchName = placementBranchFor(owner);
		if (!branchName) continue;

		// A null `repoPath` is the parked marker: this tab sits in the Global
		// Workspace because nothing claimed its cwd. Now something does, so it
		// leaves. The check has to happen BEFORE the placement below overwrites
		// the field — and it must be this field rather than "is it promoted",
		// because a tab the user promoted BY HAND is also promoted and owned, and
		// yanking that out from under them on every reconcile would be a second
		// bug wearing the first one's clothes.
		const wasParked = terminal.repoPath == null;

		const current = repositoriesStore.findOwnerForTerminal(terminalId);
		if (current?.repoPath === owner.repoPath && current.branchName === branchName) {
			// Placement already correct; the record may still be stale if the repo was
			// registered after the terminal was parked here.
			if (terminal.repoPath !== owner.repoPath) {
				terminalsStore.setRepoPath(terminalId, owner.repoPath);
				if (wasParked) globalWorkspaceStore.unpromote(terminalId);
			}
			continue;
		}

		appLogger.info(
			"terminal",
			`[Reconcile] ${terminalId} ${current ? `${current.repoPath}:${current.branchName}` : "(unplaced)"} → ${owner.repoPath}:${branchName} (cwd=${terminal.cwd})`,
		);
		batch(() => {
			if (current) repositoriesStore.removeTerminalFromBranch(current.repoPath, current.branchName, terminalId);
			terminalsStore.setRepoPath(terminalId, owner.repoPath);
			repositoriesStore.addTerminalToBranch(owner.repoPath, branchName, terminalId);
			if (wasParked) globalWorkspaceStore.unpromote(terminalId);
		});
		moved++;
	}

	if (moved > 0) appLogger.info("terminal", `[Reconcile] re-homed ${moved} terminal(s)`);
}

/**
 * What a terminal's new cwd is allowed to change about its placement: nothing,
 * unless nobody had claimed it yet.
 *
 * A `cd` is navigation, not a misplacement. The tab belongs to the repo it was
 * opened in — every deliberate placement records that owner in `repoPath` — and
 * the directory the shell happens to sit in does not revoke it. Calling the full
 * reconcile here instead moved the tab out from under the user, because the three
 * states that describe "where am I" are updated by different code and only this
 * one moved: `activeRepoPath` stayed on the old repo, so the sidebar and the tab
 * bar (which filters on it) kept showing it while the tab itself vanished from
 * the strip, and TerminalArea renders on `activeId` alone — so the pane went on
 * drawing a terminal that now belonged to a repo the user was not looking at.
 * Agents `cd` across repos constantly, which is why it read as the app switching
 * repo on its own.
 *
 * `repoPath === null` is the one case worth acting on. That tab is parked in
 * whatever repo was active because no registered repo claimed its cwd, and the
 * null records that the placement is a guess. A cd into a repo we do know answers
 * the open question, so the parked tab finally goes home.
 */
export function reclaimParkedTerminal(terminalId: string): void {
	if (terminalsStore.get(terminalId)?.repoPath != null) return;
	reconcileTerminalOwnership(terminalId);
}
