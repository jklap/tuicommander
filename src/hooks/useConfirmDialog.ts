import { createSignal } from "solid-js";
import type { BranchActivitySummary } from "../utils/activitySnapshot";

/** Which button the Enter key activates. Defaults to "confirm" for backward
 *  compatibility; destructive dialogs can set "cancel" so an accidental Enter
 *  takes the safe path instead of the primary (destructive) action. */
export type ConfirmDefaultButton = "confirm" | "cancel";

/** Outcome of a confirm dialog. "discard" is only reachable when a third
 *  (discard) button is configured, e.g. the Save / Don't Save / Cancel prompt. */
export type ConfirmResult = "confirm" | "cancel" | "discard";

export interface ConfirmOptions {
	title: string;
	message: string;
	okLabel?: string;
	cancelLabel?: string;
	/** Optional middle button (e.g. "Don't Save"). When set the dialog offers a
	 *  third outcome ("discard") between confirm and cancel. */
	discardLabel?: string;
	kind?: "info" | "warning" | "error";
	/** Which button Enter activates. Defaults to "confirm". */
	defaultButton?: ConfirmDefaultButton;
	/** When set, the dialog auto-clicks cancel after this many ms, with a countdown on the cancel label. */
	autoCancelMs?: number;
}

/** Internal state for the currently visible confirm dialog */
export interface ConfirmDialogState {
	title: string;
	message: string;
	confirmLabel: string;
	cancelLabel: string;
	discardLabel?: string;
	kind: "info" | "warning" | "error";
	defaultButton: ConfirmDefaultButton;
	autoCancelMs?: number;
}

/**
 * Hook for confirmation dialogs — renders an in-app ConfirmDialog
 * instead of native OS dialogs for consistent dark-theme styling.
 *
 * confirm() returns a Promise<boolean> that resolves when the user
 * clicks confirm or cancel (or presses Enter/Escape).
 */
export function useConfirmDialog() {
	const [dialogState, setDialogState] = createSignal<ConfirmDialogState | null>(null);
	// FIFO queue of pending confirm requests. The head is the dialog currently
	// shown. Concurrent confirm() calls enqueue instead of overwriting a single
	// resolver — the previous single-slot design orphaned every promise but the
	// last (its await never settled) and silently dropped earlier dialogs.
	const queue: Array<{ options: ConfirmOptions; resolve: (value: ConfirmResult) => void }> = [];

	/** Render the dialog at the head of the queue, or hide it when empty. */
	function showHead() {
		const head = queue[0];
		if (!head) {
			setDialogState(null);
			return;
		}
		setDialogState({
			title: head.options.title,
			message: head.options.message,
			confirmLabel: head.options.okLabel || "OK",
			cancelLabel: head.options.cancelLabel || "Cancel",
			discardLabel: head.options.discardLabel,
			kind: head.options.kind || "warning",
			defaultButton: head.options.defaultButton || "confirm",
			autoCancelMs: head.options.autoCancelMs,
		});
	}

	/** Show a confirmation dialog — resolves true on confirm, false on cancel.
	 *  When a dialog is already visible, this one queues and shows after it. */
	function confirm(options: ConfirmOptions): Promise<boolean> {
		return new Promise<boolean>((resolve) => {
			queue.push({ options, resolve: (value) => resolve(value === "confirm") });
			if (queue.length === 1) showHead();
		});
	}

	/** Resolve the current dialog with `value` and advance to the next queued one. */
	function settle(value: ConfirmResult) {
		const head = queue.shift();
		head?.resolve(value);
		showHead();
	}

	/** Called when user confirms */
	function handleConfirm() {
		settle("confirm");
	}

	/** Called when user cancels (button, Escape, or overlay click) */
	function handleClose() {
		settle("cancel");
	}

	/** Called when the user picks the middle "discard" action (e.g. Don't Save) */
	function handleDiscard() {
		settle("discard");
	}

	/** Confirm removing a worktree/branch */
	async function confirmRemoveWorktree(branchName: string): Promise<boolean> {
		return await confirm({
			title: "Remove worktree?",
			message: `Remove ${branchName}?\nThis deletes the worktree directory and its local branch.`,
			okLabel: "Remove",
			cancelLabel: "Cancel",
			kind: "warning",
		});
	}

	/** Confirm force-removing a worktree that is locked by an active agent.
	 *
	 *  `deleteBranch` only affects the message: branch deletion always uses the
	 *  safe `git branch -d` — an unmerged/unpushed branch survives even a
	 *  force-remove of its worktree — so this no longer warns about commit
	 *  loss the way it used to (see `worktree.rs::remove_worktree_by_branch`).
	 *
	 *  `isDirty` (default true — fail toward warning, not toward silence) warns
	 *  that force-removing a locked worktree uses Forced (double `--force`),
	 *  which ALSO overrides git's dirty-worktree refusal: confirming this
	 *  dialog can discard uncommitted work too, with no dirty-specific dialog
	 *  of its own in between.
	 *
	 *  `defaultButton: "cancel"` deliberately, matching its two siblings below
	 *  — pressing Enter through a queue of near-identical confirm prompts
	 *  (e.g. a batch delete) cannot destroy live work, the incident's most
	 *  plausible trigger mechanism.
	 */
	async function confirmRemoveLockedWorktree(
		branchName: string,
		deleteBranch: boolean = true,
		isDirty: boolean = true,
	): Promise<boolean> {
		const branchNote = deleteBranch
			? `\n\nThe branch "${branchName}" will only be deleted if it's fully merged (\`git branch -d\`) — an unmerged branch is kept.`
			: "";
		const dirtyWarning = isDirty
			? `\n\nThis worktree also has (or may have) uncommitted changes — forcing the removal through will discard them too.`
			: "";
		return await confirm({
			title: "Worktree is locked by an agent",
			message: `"${branchName}" is currently locked by an active Claude agent.\n\nForce-removing it may interrupt the agent mid-task.${dirtyWarning}${branchNote}\n\nContinue anyway?`,
			okLabel: "Force Remove",
			cancelLabel: "Cancel",
			kind: "warning",
			defaultButton: "cancel",
		});
	}

	/** Confirm removing a worktree with a live PTY/agent session attached.
	 *
	 *  This is the primary UI-side gate for the class of incident where a
	 *  worktree with an active (even idle) terminal got deleted out from under
	 *  it — it fires BEFORE any terminal is closed, using the branch's
	 *  attached-terminal list as of the moment the user clicked delete. See
	 *  `createWorktreeRemovalCoordinator` and
	 *  `plans/worktree-removal-incident-2026-08-26.md`.
	 *
	 *  `defaultButton: "cancel"` deliberately, so pressing Enter through a
	 *  queue of near-identical confirm prompts (e.g. a batch delete) cannot
	 *  destroy live work — the incident's most plausible trigger mechanism. */
	async function confirmRemoveBusyWorktree(branchName: string, summary: BranchActivitySummary): Promise<boolean> {
		const lines = summary.terminals.map((t) => `  • ${t.agentType ?? "terminal"} — ${t.label}`).join("\n");
		return await confirm({
			title: `"${branchName}" is in use`,
			message: `${summary.terminalCount} terminal(s) are attached to "${branchName}":\n${lines}\n\nRemoving it now will interrupt whatever is running there, and any uncommitted work may be lost.\n\nDelete anyway?`,
			okLabel: "Delete anyway",
			cancelLabel: "Cancel",
			kind: "error",
			defaultButton: "cancel",
		});
	}

	/** Confirm force-removing a worktree whose Safe removal was refused because
	 *  it has uncommitted changes (`worktree_dirty:`). */
	async function confirmForceRemoveDirtyWorktree(branchName: string): Promise<boolean> {
		return await confirm({
			title: "Uncommitted changes in the worktree",
			message: `"${branchName}"'s worktree has uncommitted changes.\n\nRemoving it now discards them permanently.\n\nDelete anyway?`,
			okLabel: "Delete anyway",
			cancelLabel: "Cancel",
			kind: "error",
			defaultButton: "cancel",
		});
	}

	/** Confirm closing a terminal */
	async function confirmCloseTerminal(terminalName: string): Promise<boolean> {
		return await confirm({
			title: "Close terminal?",
			message: `Close ${terminalName}?\nAny running processes will be terminated.`,
			okLabel: "Close",
			cancelLabel: "Cancel",
			kind: "warning",
		});
	}

	/** Prompt to save unsaved editor changes before closing a tab.
	 *  Enter defaults to Save (a safe, non-destructive action) and Escape cancels,
	 *  so an accidental keypress never discards the user's changes.
	 *  Resolves "confirm" = save then close, "discard" = close without saving,
	 *  "cancel" = keep the tab open. */
	async function confirmSaveChanges(fileName: string): Promise<ConfirmResult> {
		return new Promise<ConfirmResult>((resolve) => {
			queue.push({
				options: {
					title: "Unsaved changes",
					message: '"' + fileName + '" has unsaved changes.\nDo you want to save your changes before closing?',
					okLabel: "Save",
					discardLabel: "Don't Save",
					cancelLabel: "Cancel",
					kind: "warning",
					defaultButton: "confirm",
				},
				resolve,
			});
			if (queue.length === 1) showHead();
		});
	}

	/** Confirm removing a repository */
	async function confirmRemoveRepo(repoName: string): Promise<boolean> {
		return await confirm({
			title: "Remove repository?",
			message: `Remove ${repoName} from the list?\nThis does not delete any files.`,
			okLabel: "Remove",
			cancelLabel: "Cancel",
			kind: "warning",
		});
	}

	/** Confirm stashing changes before switching branch */
	async function confirmStashAndSwitch(branchName: string): Promise<boolean> {
		return await confirm({
			title: "Uncommitted changes",
			message: `Working tree has uncommitted changes.\nStash them and switch to ${branchName}?`,
			okLabel: "Stash & Switch",
			cancelLabel: "Cancel",
			kind: "warning",
		});
	}

	/** Report a git operation failure in a dialog, showing the full git output
	 *  (not a truncated status-line message). When `offerRetry` is true the
	 *  primary button reads "Retry" and the promise resolves true if the user
	 *  chose it; otherwise it's a plain acknowledge dialog. */
	async function reportGitError(title: string, detail: string, offerRetry = false): Promise<boolean> {
		return await confirm({
			title,
			message: detail,
			okLabel: offerRetry ? "Retry" : "OK",
			cancelLabel: offerRetry ? "Cancel" : "Dismiss",
			kind: "error",
		});
	}

	/** Confirm archiving orphaned worktrees (detached-HEAD, branch deleted). Archives rather
	 *  than deletes: orphan detection is a heuristic, so a false positive must stay recoverable. */
	async function confirmOrphanCleanup(paths: string[]): Promise<boolean> {
		const list = paths.map((p) => `  • ${p}`).join("\n");
		return await confirm({
			title: "Orphaned worktrees found",
			message: `${paths.length} worktree(s) have no branch and will be archived:\n${list}`,
			okLabel: "Archive",
			cancelLabel: "Keep",
			kind: "warning",
		});
	}

	/** Confirm a cleanup that would take the worktree's uncommitted work with it.
	 *  The backend refuses archive/delete on a worktree it cannot confirm is clean;
	 *  this is the ask that unblocks it. `commitsAhead` is 0 when the merge itself
	 *  would also be a no-op — worth saying, because then nothing is gained either. */
	async function confirmDirtyWorktreeCleanup(
		branchName: string,
		action: string,
		commitsAhead: number,
	): Promise<boolean> {
		const verb = action === "delete" ? "Deleting" : "Archiving";
		const fate =
			action === "delete"
				? "removes the worktree directory — the uncommitted changes are lost."
				: "moves the worktree to __archived/ — the uncommitted changes move with it.";
		const noop =
			commitsAhead === 0
				? `\n\n"${branchName}" also has no commits the target branch lacks, so the merge itself would do nothing.`
				: "";
		return await confirm({
			title: "Uncommitted work in the worktree",
			message: `The worktree for "${branchName}" has uncommitted changes. ${verb} it ${fate}${noop}\n\nContinue?`,
			okLabel: action === "delete" ? "Delete anyway" : "Archive anyway",
			cancelLabel: "Keep it",
			kind: "warning",
		});
	}

	return {
		confirm,
		confirmSaveChanges,
		confirmRemoveWorktree,
		confirmRemoveLockedWorktree,
		confirmRemoveBusyWorktree,
		confirmForceRemoveDirtyWorktree,
		confirmCloseTerminal,
		confirmRemoveRepo,
		confirmStashAndSwitch,
		confirmOrphanCleanup,
		confirmDirtyWorktreeCleanup,
		reportGitError,
		/** Reactive state for rendering the dialog — null when hidden */
		dialogState,
		/** Handler for confirm button / Enter key */
		handleConfirm,
		/** Handler for cancel button / Escape key / overlay click */
		handleClose,
		/** Handler for the middle discard button (e.g. Don't Save) */
		handleDiscard,
	};
}
