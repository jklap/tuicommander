import { onCleanup } from "solid-js";
import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { githubStore } from "../stores/github";
import { repoSettingsStore } from "../stores/repoSettings";
import { repositoriesStore } from "../stores/repositories";
import type { ConfirmOptions } from "./useConfirmDialog";

interface AutoDeleteDeps {
	confirm: (options: ConfirmOptions) => Promise<boolean>;
}

/**
 * Handles automatic deletion of local branches when their PR is merged or closed.
 *
 * Reads the per-repo `autoDeleteOnPrClose` setting (off/ask/auto) and:
 * - off: does nothing
 * - ask: shows a confirm dialog
 * - auto: deletes silently (falls back to ask if worktree is dirty)
 *
 * Safety: never deletes the default/main branch.
 */
export function useAutoDeleteBranch(deps: AutoDeleteDeps): void {
	/** Track processed transitions to prevent double-firing */
	const processed = new Set<string>();

	function handlePrTerminal(repoPath: string, branch: string, prNumber: number, type: "merged" | "closed"): void {
		const key = `${repoPath}:${prNumber}`;
		if (processed.has(key)) return;
		processed.add(key);

		// Don't block the polling loop — run async
		processAutoDelete(repoPath, branch, prNumber, type).catch((err) =>
			appLogger.warn("git", `Auto-delete failed for ${branch}`, err),
		);
	}

	async function processAutoDelete(
		repoPath: string,
		branch: string,
		prNumber: number,
		type: "merged" | "closed",
	): Promise<void> {
		// Check setting
		const effective = repoSettingsStore.getEffective(repoPath);
		const mode = effective?.autoDeleteOnPrClose ?? "off";
		if (mode === "off") return;

		// A closed PR names its head BRANCH, so the row it belongs to comes from the
		// single branch->id seam (see its DEFERRED note: with two workspaces on one
		// branch, a PR cannot say which checkout to delete).
		const repo = repositoriesStore.get(repoPath);
		const workspaceId = repositoriesStore.workspaceIdOnBranch(repoPath, branch);
		if (repo) {
			const branchState = workspaceId ? repo.workspaces[workspaceId] : undefined;
			if (branchState?.isMain) {
				appLogger.debug("git", `Skipping auto-delete for default branch '${branch}'`);
				return;
			}
		}

		// No workspace has it checked out — nothing local to delete.
		if (repo && !workspaceId) return;

		let effectiveMode = mode;

		// If auto mode, check dirty state first
		if (effectiveMode === "auto") {
			try {
				const dirty = await invoke<boolean>("check_worktree_dirty", {
					repoPath,
					workspaceId: workspaceId ?? branch,
				});
				if (dirty) {
					appLogger.info("git", `Branch '${branch}' has uncommitted changes — asking before deleting`);
					effectiveMode = "ask";
				}
			} catch {
				// If dirty check fails, fall back to ask
				effectiveMode = "ask";
			}
		}

		if (effectiveMode === "ask") {
			const action = type === "merged" ? "merged" : "closed";
			const confirmed = await deps.confirm({
				title: "Delete local branch?",
				message: `PR #${prNumber} was ${action}.\nDelete local branch '${branch}'?`,
				okLabel: "Delete",
				cancelLabel: "Keep",
				kind: "warning",
			});
			if (!confirmed) return;
		}

		// Perform deletion
		try {
			await invoke("delete_local_branch", { repoPath, branchName: branch, workspaceId: branch });
			repositoriesStore.bumpGitRevision(repoPath);
			appLogger.info("git", `Auto-deleted branch '${branch}' (PR #${prNumber} ${type})`);
		} catch (err) {
			appLogger.warn("git", `Failed to delete branch '${branch}': ${err}`);
		}
	}

	githubStore.setOnPrTerminal(handlePrTerminal);

	onCleanup(() => {
		githubStore.setOnPrTerminal(null);
		processed.clear();
	});
}
