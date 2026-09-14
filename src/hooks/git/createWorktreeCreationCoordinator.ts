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

/** How long to wait for the backend's post-create chain (CoW warm, then the
 * file sync, then the Setup Script) before giving up and letting the Run
 * Script proceed anyway. Must exceed the backend's own fixed 900 s script
 * deadline (tuic-git `SCRIPT_TIMEOUT`) with real headroom, since the wait also
 * covers the warm and the sync that precede the script. */
export const SETUP_SCRIPT_WAIT_TIMEOUT_MS = 1_200_000;

/** A subscription to `worktree-setup-script-completed` for one repo, opened
 *  BEFORE the creation request so a chain that finishes quickly cannot report
 *  before anyone listens. */
export interface SetupScriptWaiter {
	/** Resolve once the event for `branch` arrived (also if it already did) or
	 *  the safety-net timeout elapsed. Never rejects. */
	wait: (branch: string) => Promise<void>;
	/** Stop listening without waiting (the creation request failed). */
	cancel: () => void;
}

/** Arm a [`SetupScriptWaiter`] for `repoPath`. The backend runs the Setup
 * Script in its own background chain (`worktree::spawn_worktree_setup_chain`),
 * so without waiting the Run Script typed into the new terminal could race it
 * (`npm run dev` before `npm install` finished). The timeout guarantees a lost
 * event — backend crash, SSE disconnect — cannot hang creation forever. */
export function armSetupScriptWaiter(repoPath: string, timeoutMs = SETUP_SCRIPT_WAIT_TIMEOUT_MS): SetupScriptWaiter {
	const seen = new Set<string>();
	let wanted: { branch: string; resolve: () => void } | null = null;
	let timer: ReturnType<typeof setTimeout> | undefined;
	let unlisten: (() => void) | undefined;
	let closed = false;
	const close = () => {
		if (closed) return;
		closed = true;
		if (timer !== undefined) clearTimeout(timer);
		unlisten?.();
	};
	listen<{ repoPath: string; branch: string }>("worktree-setup-script-completed", (event) => {
		if (closed || event.payload.repoPath !== repoPath) return;
		seen.add(event.payload.branch);
		if (wanted && wanted.branch === event.payload.branch) {
			const { resolve } = wanted;
			wanted = null;
			close();
			resolve();
		}
	})
		.then((fn) => {
			unlisten = fn;
			if (closed) fn();
		})
		.catch((err) => appLogger.warn("git", "Failed to listen for worktree-setup-script-completed", err));
	return {
		wait: (branch) =>
			new Promise<void>((resolve) => {
				if (closed || seen.has(branch)) {
					close();
					resolve();
					return;
				}
				wanted = { branch, resolve };
				timer = setTimeout(() => {
					appLogger.warn("git", `Gave up waiting for the setup script in ${branch} after ${timeoutMs} ms`);
					wanted = null;
					close();
					resolve();
				}, timeoutMs);
			}),
		cancel: close,
	};
}

/** How `setupNewWorktree` treats a configured Setup Script. */
export interface SetupNewWorktreeOptions {
	/** Pre-armed waiter for a worktree the backend chain is setting up. When
	 *  absent for a backend-created worktree, one is armed late (may miss a
	 *  very fast chain; then the timeout releases the wait). */
	setupWaiter?: SetupScriptWaiter;
	/** The worktree was NOT created through `create_worktree` (e.g. conflict
	 *  assist), so no backend chain runs its Setup Script — run it from here. */
	runSetupScriptHere?: boolean;
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
		) => Promise<PendingCreation["result"] & { status: "ok" }>;
		runSetupScript: (script: string, cwd: string) => Promise<{ exit_code: number; stdout: string; stderr: string }>;
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
		options: SetupNewWorktreeOptions = {},
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

		// A backend-created worktree's Setup Script runs in the backend's
		// background chain (worktree::spawn_worktree_setup_chain: CoW warm ->
		// file sync -> script), so it can never race a synced file it depends
		// on; its outcome is reported by useAppInit.ts's
		// "worktree-setup-script-completed" listener. We still WAIT for that
		// event here before creating the terminal, so the Run Script cannot
		// start before the Setup Script finished (`npm run dev` racing `npm
		// install`) — the guarantee the old inline runSetupScript call gave.
		const effective = repoSettingsStore.getEffective(repoPath);
		if (effective?.setupScript) {
			if (options.runSetupScriptHere) {
				try {
					deps.setStatusInfo(`Running setup script in ${displayName}...`);
					const scriptResult = await deps.repo.runSetupScript(effective.setupScript, result.path);
					if (scriptResult.exit_code !== 0) {
						appLogger.warn("git", `Setup script failed (exit ${scriptResult.exit_code})`, scriptResult.stderr);
						deps.setStatusInfo(`Setup script failed (exit ${scriptResult.exit_code})`);
					}
				} catch (err) {
					appLogger.warn("git", "Setup script execution error", err);
					deps.setStatusInfo(`Setup script failed: ${err}`);
				}
			} else {
				deps.setStatusInfo(`Running setup script in ${displayName}...`);
				await (options.setupWaiter ?? armSetupScriptWaiter(repoPath)).wait(result.branch);
			}
		} else {
			options.setupWaiter?.cancel();
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

		// Armed before the request: the backend chain may finish its Setup
		// Script before createWorktree's response is processed.
		const setupWaiter = repoSettingsStore.getEffective(repoPath)?.setupScript
			? armSetupScriptWaiter(repoPath)
			: undefined;
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

			await setupNewWorktree(repoPath, result, options.branchName, undefined, { setupWaiter });
		} catch (err) {
			setupWaiter?.cancel();
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
		let setupWaiter: SetupScriptWaiter | undefined;

		try {
			const repoState = repositoriesStore.get(repoPath);
			const existingBranches = repoState ? Object.keys(repoState.workspaces) : [];
			const cloneName = await deps.repo.generateCloneBranchName(branchName, existingBranches);

			deps.setStatusInfo(`Creating worktree ${cloneName}...`);
			setupWaiter = repoSettingsStore.getEffective(repoPath)?.setupScript ? armSetupScriptWaiter(repoPath) : undefined;
			const result = await deps.repo.createWorktree(repoPath, cloneName, true, branchName);

			await setupNewWorktree(repoPath, result, cloneName, undefined, { setupWaiter });
		} catch (err) {
			setupWaiter?.cancel();
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
