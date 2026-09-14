import type { Accessor, Setter } from "solid-js";
import { AGENTS } from "../../agents";
import type { WorktreeCreateOptions } from "../../components/CreateWorktreeDialog";
import { listen } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { repoSettingsStore } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import type { BaseRefOption } from "../useRepository";
import type { AgentSeed } from "./agentSeed";
import type { PendingCreation } from "./createRepositoryRefreshCoordinator";

/** How long to wait for the backend's setup-script chain (sync, then the script
 * itself) before giving up and letting the Run Script proceed anyway — must
 * exceed the backend's own setup-script timeout (600s default,
 * `RepoDefaultsConfig.setupScriptTimeoutSecs`) with real headroom, since the
 * wait also covers the file sync that precedes the script. */
const SETUP_SCRIPT_WAIT_TIMEOUT_MS = 900_000;

/** Waits for the `worktree-setup-script-completed` event matching this
 * `repoPath`/`branch`, so the Run Script (typed into the terminal right after
 * this resolves) can't race the Setup Script the backend runs in its own
 * background chain (`worktree::spawn_worktree_setup_chain`) — without this,
 * e.g. `npm run dev` could start before `npm install` finished. Resolves
 * (never rejects) either on the matching event or after `timeoutMs`, so a
 * lost event — the backend crashing, an SSE disconnect — can't hang worktree
 * creation forever; the caller proceeds either way. */
function waitForSetupScriptCompletion(
	repoPath: string,
	branch: string,
	timeoutMs = SETUP_SCRIPT_WAIT_TIMEOUT_MS,
): Promise<void> {
	return new Promise((resolve) => {
		let settled = false;
		let unlisten: (() => void) | undefined;
		const timer = setTimeout(() => {
			if (settled) return;
			settled = true;
			unlisten?.();
			resolve();
		}, timeoutMs);
		listen<{ repoPath: string; branch: string }>("worktree-setup-script-completed", (event) => {
			if (settled) return;
			if (event.payload.repoPath !== repoPath || event.payload.branch !== branch) return;
			settled = true;
			clearTimeout(timer);
			unlisten?.();
			resolve();
		}).then((fn) => {
			unlisten = fn;
			if (settled) fn();
		});
	});
}

export interface WorktreeDialogState {
	repoPath: string;
	suggestedName: string;
	existingBranches: string[];
	worktreeBranches: string[];
	worktreesDir: string;
	baseRefs: BaseRefOption[];
	/** Base ref to preselect in the dialog — the last one used successfully for this
	 * repo this session, falling back to the repo's configured "Branch From" setting,
	 * then to `baseRefs[0]`. Empty when the configured setting is stale (see
	 * `missingBaseBranch`) and nothing else applies. */
	defaultBaseRef: string;
	/** Set when the repo's configured "Branch From" setting names a branch that is no
	 * longer in `baseRefs` (deleted since it was set) AND nothing else (session memory)
	 * resolved a default instead. The dialog surfaces this as a non-blocking warning;
	 * it must never prevent creating a worktree. */
	missingBaseBranch?: string;
}

interface WorktreeCreationCoordinatorDeps {
	repo: {
		generateWorktreeName: (existingNames: string[]) => Promise<string>;
		generateCloneBranchName: (sourceBranch: string, existingNames: string[]) => Promise<string>;
		listLocalBranches: (repoPath: string) => Promise<string[]>;
		listBaseRefOptions: (repoPath: string) => Promise<BaseRefOption[]>;
		createWorktree: (
			baseRepo: string,
			branchName: string,
			createBranch?: boolean,
			baseRef?: string,
		) => Promise<PendingCreation["result"] & { status: "ok" | "pending" }>;
		getDiffStats: (path: string) => Promise<{ additions: number; deletions: number }>;
	};
	pty: {
		getWorktreesDir: (repoPath?: string) => Promise<string>;
	};
	getPromptOnCreate?: (repoPath: string) => boolean;
	setStatusInfo: (message: string) => void;
	creatingWorktreeRepos: Accessor<Set<string>>;
	setCreatingWorktreeRepos: Setter<Set<string>>;
	worktreeDialogState: Accessor<WorktreeDialogState | null>;
	setWorktreeDialogState: Setter<WorktreeDialogState | null>;
	markRecentlyCreated: (repoPath: string, workspaceId: string) => void;
	handleAddTerminalToWorkspace: (repoPath: string, workspaceId: string) => Promise<string | undefined>;
}

/** Owns worktree creation, setup, and initial terminal seeding. */
export function createWorktreeCreationCoordinator(deps: WorktreeCreationCoordinatorDeps) {
	const {
		creatingWorktreeRepos,
		setCreatingWorktreeRepos,
		worktreeDialogState,
		setWorktreeDialogState,
		markRecentlyCreated,
		handleAddTerminalToWorkspace,
	} = deps;

	// Last base ref successfully used per repo, this session only — forgotten on
	// restart (repo-level persisted settings are a heavier bar for what is really
	// just a UI convenience). Recorded only on a successful creation FROM THE
	// DIALOG; see confirmCreateWorktree below.
	const lastBaseRefByRepo = new Map<string, string>();

	/** Base ref to preselect: the remembered one if it still exists among `baseRefs`; else
	 * the repo's configured "Branch From" setting if it's a concrete branch that still
	 * exists; else the backend's own default (`baseRefs[0]`). A configured setting that no
	 * longer exists is reported via `missingBaseBranch` rather than silently ignored — it
	 * never blocks resolving *some* default, just flags that the setting needs a look. */
	const resolveDefaultBaseRef = (
		repoPath: string,
		baseRefs: BaseRefOption[],
	): { defaultBaseRef: string; missingBaseBranch?: string } => {
		const remembered = lastBaseRefByRepo.get(repoPath);
		if (remembered && baseRefs.some((r) => r.name === remembered)) {
			return { defaultBaseRef: remembered };
		}

		const configured = repoSettingsStore.getEffectiveField(repoPath, "baseBranch");
		if (configured && configured !== "automatic") {
			if (baseRefs.some((r) => r.name === configured)) {
				return { defaultBaseRef: configured };
			}
			return { defaultBaseRef: baseRefs[0]?.name ?? "", missingBaseBranch: configured };
		}

		return { defaultBaseRef: baseRefs[0]?.name ?? "" };
	};

	const handleAddWorktree = async (repoPath: string) => {
		// Prevent concurrent creations for the same repo
		if (creatingWorktreeRepos().has(repoPath)) return;

		const repoState = repositoriesStore.get(repoPath);
		const worktreeBranches = repoState ? Object.keys(repoState.workspaces) : [];

		// Fetch data for the dialog in parallel
		const [suggestedName, localBranches, worktreesDir, baseRefs] = await Promise.all([
			deps.repo.generateWorktreeName(worktreeBranches),
			deps.repo.listLocalBranches(repoPath),
			deps.pty.getWorktreesDir(repoPath),
			deps.repo.listBaseRefOptions(repoPath),
		]);

		const { defaultBaseRef, missingBaseBranch } = resolveDefaultBaseRef(repoPath, baseRefs);
		const promptOnCreate = deps.getPromptOnCreate?.(repoPath) ?? true;

		if (missingBaseBranch) {
			appLogger.warn(
				"git",
				`Configured "Branch From" setting "${missingBaseBranch}" no longer exists in ${repoPath} — falling back`,
			);
		}

		if (!promptOnCreate) {
			// Skip dialog: create worktree instantly with auto-generated name. A stale
			// configured setting still falls back to defaultBaseRef (baseRefs[0]) here —
			// there's no dialog to show the warning in, so it's surfaced via status instead.
			if (missingBaseBranch) {
				deps.setStatusInfo(
					`Configured base branch "${missingBaseBranch}" no longer exists — using "${defaultBaseRef}" instead`,
				);
			}
			setWorktreeDialogState({
				repoPath,
				suggestedName,
				existingBranches: localBranches,
				worktreeBranches,
				worktreesDir,
				baseRefs,
				defaultBaseRef,
				missingBaseBranch,
			});
			await confirmCreateWorktree({
				branchName: suggestedName,
				createBranch: true,
				baseRef: defaultBaseRef || "HEAD",
			});
			return;
		}

		setWorktreeDialogState({
			repoPath,
			suggestedName,
			existingBranches: localBranches,
			worktreeBranches,
			worktreesDir,
			baseRefs,
			defaultBaseRef,
			missingBaseBranch,
		});
	};

	/** Shared post-creation setup: run scripts, open terminal, fetch stats */
	const setupNewWorktree = async (
		repoPath: string,
		result: PendingCreation["result"],
		displayName: string,
		agentSeed?: AgentSeed,
	) => {
		// Keyed by the id the backend reported; `branch` is display data.
		markRecentlyCreated(repoPath, result.workspace_id);
		repositoriesStore.setWorkspace(repoPath, result.workspace_id, {
			branchName: result.branch,
			worktreePath: result.path,
			kind: result.kind ?? "worktree",
			parentRepoPath: null,
		});
		repositoriesStore.setActiveWorkspace(repoPath, result.workspace_id);

		// The setup script (if configured) is no longer run from here — the
		// backend now chains it after the worktree file sync, in the
		// background (worktree::spawn_worktree_setup_chain), so it can't
		// race a copy_ignored_files/copy_untracked_files/copy_paths sync the
		// script might depend on. Its outcome (if any script is configured)
		// arrives via the "worktree-setup-script-completed" event — see
		// useAppInit.ts's listener — which handles logging/status-reporting.
		// We still WAIT for that event here (with a generous timeout) before
		// creating the terminal / queuing the Run Script — without this, the
		// Run Script can start concurrently with (or before) the Setup
		// Script, e.g. `npm run dev` racing `npm install`. This restores the
		// ordering guarantee the old inline `await runSetupScript(...)` call
		// used to provide, without reintroducing a synchronous script-execution
		// IPC call here.
		const effective = repoSettingsStore.getEffective(repoPath);
		if (effective?.setupScript) {
			await waitForSetupScriptCompletion(repoPath, result.branch);
		}

		const termId = await handleAddTerminalToWorkspace(repoPath, result.workspace_id);

		// Seed must be applied HERE (synchronously after terminal creation, before
		// the getDiffStats await below) — Terminal.tsx reads agentType/pendingInitCommand
		// when it creates the PTY (passes agent_type only if pendingInitCommand is set),
		// which fires on the next rAF. Setting it after setupNewWorktree returns would
		// race that rAF. Same proven window the runScript branch uses.
		if (termId && agentSeed) {
			terminalsStore.update(termId, {
				agentType: agentSeed.agentType,
				pendingInitCommand: agentSeed.initCommand,
				agentLaunchCommand: agentSeed.launchCommand,
				name: AGENTS[agentSeed.agentType].name,
				nameIsCustom: true,
			});
		} else if (termId && effective?.runScript) {
			terminalsStore.update(termId, { pendingInitCommand: effective.runScript });
		}

		try {
			const stats = await deps.repo.getDiffStats(result.path);
			repositoriesStore.updateWorkspaceStats(repoPath, result.workspace_id, stats.additions, stats.deletions);
		} catch (err) {
			appLogger.debug("git", `getDiffStats failed for ${result.branch}`, err);
		}

		deps.setStatusInfo(`Created worktree ${displayName}`);
	};

	const confirmCreateWorktree = async (options: WorktreeCreateOptions) => {
		const dialogState = worktreeDialogState();
		if (!dialogState) return;

		const { repoPath } = dialogState;

		if (creatingWorktreeRepos().has(repoPath)) return;
		setCreatingWorktreeRepos((prev) => new Set([...prev, repoPath]));

		try {
			deps.setStatusInfo(`Creating worktree ${options.branchName}...`);
			// An empty baseRef (nothing selected — e.g. a stale configured branch left the
			// dialog with no preselection) must reach the backend as "no base ref" (branch
			// from HEAD), not as a literal empty-string start-point argument.
			const result = await deps.repo.createWorktree(
				repoPath,
				options.branchName,
				options.createBranch,
				options.baseRef || undefined,
			);

			// Remember this repo's base ref for next time — only on success (never in
			// the catch below), and only for "pending" or "ok": both mean the backend
			// accepted the ref. Deliberately NOT done in handleCreateWorktreeFromBranch —
			// that quick-clone flow's base ref is whichever branch was right-clicked, not
			// a choice made in this dialog, so it shouldn't redefine the dialog's default.
			if (options.baseRef) lastBaseRefByRepo.set(repoPath, options.baseRef);

			setWorktreeDialogState(null);

			await setupNewWorktree(repoPath, result, options.branchName);
		} catch (err) {
			appLogger.error("git", "Failed to create worktree", err);
			deps.setStatusInfo(`Failed to create worktree: ${err}`);
			// Re-throw so the dialog can show the error and stay open
			throw err;
		} finally {
			setCreatingWorktreeRepos((prev) => {
				const next = new Set(prev);
				next.delete(repoPath);
				return next;
			});
		}
	};

	/** Quick-clone flow: right-click branch → instant worktree with hybrid name */
	const handleCreateWorktreeFromBranch = async (repoPath: string, branchName: string) => {
		if (creatingWorktreeRepos().has(repoPath)) return;
		setCreatingWorktreeRepos((prev) => new Set([...prev, repoPath]));

		try {
			const repoState = repositoriesStore.get(repoPath);
			const existingBranches = repoState ? Object.keys(repoState.workspaces) : [];
			const cloneName = await deps.repo.generateCloneBranchName(branchName, existingBranches);

			deps.setStatusInfo(`Creating worktree ${cloneName}...`);
			const result = await deps.repo.createWorktree(repoPath, cloneName, true, branchName);

			await setupNewWorktree(repoPath, result, cloneName);
		} catch (err) {
			appLogger.error("git", "Failed to create worktree from branch", err);
			deps.setStatusInfo(`Failed to create worktree: ${err}`);
		} finally {
			setCreatingWorktreeRepos((prev) => {
				const next = new Set(prev);
				next.delete(repoPath);
				return next;
			});
		}
	};

	return {
		confirmCreateWorktree,
		handleAddWorktree,
		handleCreateWorktreeFromBranch,
		setupNewWorktree,
	};
}
