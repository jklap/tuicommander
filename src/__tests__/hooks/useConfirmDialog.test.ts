import { beforeEach, describe, expect, it } from "vitest";
import { useConfirmDialog } from "../../hooks/useConfirmDialog";

describe("useConfirmDialog", () => {
	let dialog: ReturnType<typeof useConfirmDialog>;

	beforeEach(() => {
		dialog = useConfirmDialog();
	});

	describe("confirm()", () => {
		it("sets dialogState when called and resolves true on confirm", async () => {
			expect(dialog.dialogState()).toBe(null);

			const promise = dialog.confirm({
				title: "Delete?",
				message: "Are you sure?",
				okLabel: "Yes",
				cancelLabel: "No",
				kind: "warning",
			});

			expect(dialog.dialogState()).toEqual({
				title: "Delete?",
				message: "Are you sure?",
				confirmLabel: "Yes",
				cancelLabel: "No",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleConfirm();
			const result = await promise;

			expect(result).toBe(true);
			expect(dialog.dialogState()).toBe(null);
		});

		it("resolves false on close", async () => {
			const promise = dialog.confirm({
				title: "Delete?",
				message: "Are you sure?",
			});

			dialog.handleClose();
			const result = await promise;

			expect(result).toBe(false);
			expect(dialog.dialogState()).toBe(null);
		});

		it("uses default okLabel, cancelLabel, and kind when not specified", async () => {
			const promise = dialog.confirm({
				title: "Confirm",
				message: "Proceed?",
			});

			expect(dialog.dialogState()).toEqual({
				title: "Confirm",
				message: "Proceed?",
				confirmLabel: "OK",
				cancelLabel: "Cancel",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleClose();
			await promise;
		});

		it("threads autoCancelMs into dialogState when provided", async () => {
			const promise = dialog.confirm({
				title: "Switch to new worktree?",
				message: "Switch now?",
				cancelLabel: "Stay",
				kind: "info",
				autoCancelMs: 10_000,
			});

			expect(dialog.dialogState()?.autoCancelMs).toBe(10_000);

			dialog.handleClose();
			await promise;
		});

		it("leaves autoCancelMs undefined when not provided", async () => {
			const promise = dialog.confirm({ title: "Confirm", message: "Proceed?" });

			expect(dialog.dialogState()?.autoCancelMs).toBeUndefined();

			dialog.handleClose();
			await promise;
		});
	});

	describe("concurrent confirm() calls", () => {
		it("queues a second confirm and shows dialogs sequentially (FIFO)", async () => {
			const first = dialog.confirm({ title: "First", message: "1?" });
			const second = dialog.confirm({ title: "Second", message: "2?" });

			// Only the first is shown; the second waits in the queue.
			expect(dialog.dialogState()?.title).toBe("First");

			dialog.handleConfirm();
			expect(await first).toBe(true);

			// Resolving the first advances to the queued second.
			expect(dialog.dialogState()?.title).toBe("Second");

			dialog.handleClose();
			expect(await second).toBe(false);
			expect(dialog.dialogState()).toBe(null);
		});

		it("does not orphan the first promise when a second confirm arrives", async () => {
			// Regression: the old single-slot pendingResolve was overwritten by the
			// second confirm(), so the first promise never settled. If that bug
			// returns, `await first` below hangs and the test fails via timeout.
			const first = dialog.confirm({ title: "First", message: "1?" });
			const second = dialog.confirm({ title: "Second", message: "2?" });

			dialog.handleConfirm(); // settle the head (first)
			expect(await first).toBe(true);

			// Drain the queued second so the test leaves no pending promise.
			dialog.handleClose();
			expect(await second).toBe(false);
		});

		it("resolves all three queued confirms in order", async () => {
			const results: boolean[] = [];
			const a = dialog.confirm({ title: "A", message: "?" }).then((v) => results.push(v));
			const b = dialog.confirm({ title: "B", message: "?" }).then((v) => results.push(v));
			const c = dialog.confirm({ title: "C", message: "?" }).then((v) => results.push(v));

			expect(dialog.dialogState()?.title).toBe("A");
			dialog.handleConfirm(); // A -> true
			await a;
			expect(dialog.dialogState()?.title).toBe("B");
			dialog.handleClose(); // B -> false
			await b;
			expect(dialog.dialogState()?.title).toBe("C");
			dialog.handleConfirm(); // C -> true
			await c;

			expect(results).toEqual([true, false, true]);
			expect(dialog.dialogState()).toBe(null);
		});
	});

	describe("confirmRemoveWorktree()", () => {
		it("shows dialog with correct message and resolves true on confirm", async () => {
			const promise = dialog.confirmRemoveWorktree("feature-x");

			expect(dialog.dialogState()).toEqual({
				title: "Remove worktree?",
				message: "Remove feature-x?\nThis deletes the worktree directory and its local branch.",
				confirmLabel: "Remove",
				cancelLabel: "Cancel",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("returns false when user cancels", async () => {
			const promise = dialog.confirmRemoveWorktree("feature-y");
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("confirmRemoveLockedWorktree()", () => {
		it("warns about a safe branch delete when deleteBranch=true", async () => {
			const promise = dialog.confirmRemoveLockedWorktree("feature-x", true);

			const state = dialog.dialogState();
			expect(state?.title).toBe("Worktree is locked by an agent");
			expect(state?.message).toContain('"feature-x" is currently locked by an active Claude agent.');
			expect(state?.message).toContain("may interrupt the agent mid-task");
			// Branch deletion never escalates to `-D` — this must not claim
			// unmerged commits will be lost (root cause of the 2026-08-26
			// incident's orphaned commits).
			expect(state?.message).not.toContain("-D");
			expect(state?.message).not.toContain("permanently lost");
			expect(state?.message).toContain("git branch -d");
			expect(state?.confirmLabel).toBe("Force Remove");
			// Enter must not destroy live work by default (this is the incident's
			// most plausible trigger mechanism) — same invariant as its two
			// siblings below.
			expect(state?.defaultButton).toBe("cancel");

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("omits the branch-deletion note when deleteBranch=false", async () => {
			const promise = dialog.confirmRemoveLockedWorktree("feature-x", false);

			expect(dialog.dialogState()?.message).not.toContain("git branch -d");

			dialog.handleClose();
			expect(await promise).toBe(false);
		});

		it("defaults deleteBranch to true when omitted", async () => {
			const promise = dialog.confirmRemoveLockedWorktree("feature-x");

			expect(dialog.dialogState()?.message).toContain("git branch -d");

			dialog.handleClose();
			await promise;
		});

		it("warns that uncommitted work will also be discarded when isDirty is true", async () => {
			// Forcing a locked worktree through uses double --force, which also
			// overrides git's dirty-worktree refusal — the caller must be told,
			// or confirming this dialog can silently discard uncommitted work
			// with no dirty-specific warning of its own in between.
			const promise = dialog.confirmRemoveLockedWorktree("feature-x", true, true);

			expect(dialog.dialogState()?.message).toContain("uncommitted changes");
			expect(dialog.dialogState()?.message).toContain("discard them too");

			dialog.handleClose();
			await promise;
		});

		it("omits the dirty warning when isDirty is false", async () => {
			const promise = dialog.confirmRemoveLockedWorktree("feature-x", true, false);

			expect(dialog.dialogState()?.message).not.toContain("uncommitted changes");

			dialog.handleClose();
			await promise;
		});

		it("defaults isDirty to true (fail toward warning) when omitted", async () => {
			const promise = dialog.confirmRemoveLockedWorktree("feature-x");

			expect(dialog.dialogState()?.message).toContain("uncommitted changes");

			dialog.handleClose();
			await promise;
		});
	});

	describe("confirmRemoveBusyWorktree()", () => {
		it("names the attached terminals and defaults Enter to Cancel", async () => {
			const promise = dialog.confirmRemoveBusyWorktree("feature-x", {
				terminalCount: 2,
				isBusy: true,
				terminals: [
					{ id: "t1", agentType: "claude", label: "Working" },
					{ id: "t2", agentType: null, label: "Idle" },
				],
			});

			const state = dialog.dialogState();
			expect(state?.title).toBe('"feature-x" is in use');
			expect(state?.message).toContain("2 terminal(s)");
			expect(state?.message).toContain("claude — Working");
			expect(state?.message).toContain("terminal — Idle");
			expect(state?.confirmLabel).toBe("Delete anyway");
			expect(state?.kind).toBe("error");
			// A batch delete queues one of these per busy item — Enter must not
			// destroy live work by default (this is the incident's most plausible
			// trigger mechanism: clicking/pressing through a stack of prompts).
			expect(state?.defaultButton).toBe("cancel");

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("returns false when the user cancels", async () => {
			const promise = dialog.confirmRemoveBusyWorktree("feature-y", {
				terminalCount: 1,
				isBusy: true,
				terminals: [{ id: "t1", agentType: null, label: "Idle" }],
			});
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("confirmForceRemoveDirtyWorktree()", () => {
		it("warns that uncommitted changes will be discarded and defaults Enter to Cancel", async () => {
			const promise = dialog.confirmForceRemoveDirtyWorktree("feature-x");

			const state = dialog.dialogState();
			expect(state?.title).toBe("Uncommitted changes in the worktree");
			expect(state?.message).toContain("feature-x");
			expect(state?.message).toContain("discards them permanently");
			expect(state?.confirmLabel).toBe("Delete anyway");
			expect(state?.kind).toBe("error");
			expect(state?.defaultButton).toBe("cancel");

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("returns false when the user cancels", async () => {
			const promise = dialog.confirmForceRemoveDirtyWorktree("feature-y");
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("confirmStashAndSwitch()", () => {
		it("shows dialog with correct message and resolves true on confirm", async () => {
			const promise = dialog.confirmStashAndSwitch("main");

			expect(dialog.dialogState()).toEqual({
				title: "Uncommitted changes",
				message: "Working tree has uncommitted changes.\nStash them and switch to main?",
				confirmLabel: "Stash & Switch",
				cancelLabel: "Cancel",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("returns false when user cancels", async () => {
			const promise = dialog.confirmStashAndSwitch("main");
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("reportGitError()", () => {
		it("shows OK/Dismiss labels by default", async () => {
			const promise = dialog.reportGitError("Commit failed", "fatal: nothing to commit");

			expect(dialog.dialogState()).toEqual({
				title: "Commit failed",
				message: "fatal: nothing to commit",
				confirmLabel: "OK",
				cancelLabel: "Dismiss",
				kind: "error",
				defaultButton: "confirm",
			});

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("shows Retry/Cancel labels and resolves true only when Retry is chosen", async () => {
			const promise = dialog.reportGitError("Push failed", "fatal: connection reset", true);

			const state = dialog.dialogState();
			expect(state?.confirmLabel).toBe("Retry");
			expect(state?.cancelLabel).toBe("Cancel");

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("resolves false when the acknowledge dialog is dismissed", async () => {
			const promise = dialog.reportGitError("Commit failed", "fatal: nothing to commit");
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("confirmOrphanCleanup()", () => {
		it("lists every orphan path and resolves true on Archive", async () => {
			const promise = dialog.confirmOrphanCleanup(["/repo__wt/old-a", "/repo__wt/old-b"]);

			const state = dialog.dialogState();
			expect(state?.title).toBe("Orphaned worktrees found");
			expect(state?.message).toContain("2 worktree(s)");
			expect(state?.message).toContain("/repo__wt/old-a");
			expect(state?.message).toContain("/repo__wt/old-b");
			expect(state?.confirmLabel).toBe("Archive");
			expect(state?.cancelLabel).toBe("Keep");

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("returns false when the user keeps the orphans", async () => {
			const promise = dialog.confirmOrphanCleanup(["/repo__wt/old-a"]);
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("confirmDirtyWorktreeCleanup()", () => {
		it("describes deletion and mentions the merge would be a no-op when commitsAhead is 0", async () => {
			const promise = dialog.confirmDirtyWorktreeCleanup("feature-x", "delete", 0);

			const state = dialog.dialogState();
			expect(state?.title).toBe("Uncommitted work in the worktree");
			expect(state?.message).toContain("Deleting it removes the worktree directory");
			expect(state?.message).toContain("no commits the target branch lacks");
			expect(state?.confirmLabel).toBe("Delete anyway");
			expect(state?.cancelLabel).toBe("Keep it");

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("describes archiving and omits the no-op note when there are commits ahead", async () => {
			const promise = dialog.confirmDirtyWorktreeCleanup("feature-y", "archive", 3);

			const state = dialog.dialogState();
			expect(state?.message).toContain("Archiving it moves the worktree to __archived/");
			expect(state?.message).not.toContain("would do nothing");
			expect(state?.confirmLabel).toBe("Archive anyway");

			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("confirmCloseTerminal()", () => {
		it("shows dialog with correct message for terminal name", async () => {
			const promise = dialog.confirmCloseTerminal("Terminal 1");

			expect(dialog.dialogState()).toEqual({
				title: "Close terminal?",
				message: "Close Terminal 1?\nAny running processes will be terminated.",
				confirmLabel: "Close",
				cancelLabel: "Cancel",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("returns false when user cancels", async () => {
			const promise = dialog.confirmCloseTerminal("Terminal 2");
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("confirmRemoveRepo()", () => {
		it("shows dialog with correct message for repo name", async () => {
			const promise = dialog.confirmRemoveRepo("my-repo");

			expect(dialog.dialogState()).toEqual({
				title: "Remove repository?",
				message: "Remove my-repo from the list?\nThis does not delete any files.",
				confirmLabel: "Remove",
				cancelLabel: "Cancel",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleConfirm();
			expect(await promise).toBe(true);
		});

		it("returns false when user cancels", async () => {
			const promise = dialog.confirmRemoveRepo("other-repo");
			dialog.handleClose();
			expect(await promise).toBe(false);
		});
	});

	describe("confirmSaveChanges()", () => {
		it("offers Save / Don't Save / Cancel with Save as the Enter default", () => {
			void dialog.confirmSaveChanges("notes.md");

			expect(dialog.dialogState()).toEqual({
				title: "Unsaved changes",
				message: '"notes.md" has unsaved changes.\nDo you want to save your changes before closing?',
				confirmLabel: "Save",
				cancelLabel: "Cancel",
				discardLabel: "Don't Save",
				kind: "warning",
				defaultButton: "confirm",
			});

			dialog.handleClose();
		});

		it("resolves 'confirm' when the user chooses Save", async () => {
			const promise = dialog.confirmSaveChanges("a.ts");
			dialog.handleConfirm();
			expect(await promise).toBe("confirm");
		});

		it("resolves 'discard' when the user chooses Don't Save", async () => {
			const promise = dialog.confirmSaveChanges("a.ts");
			dialog.handleDiscard();
			expect(await promise).toBe("discard");
		});

		it("resolves 'cancel' when the user cancels", async () => {
			const promise = dialog.confirmSaveChanges("a.ts");
			dialog.handleClose();
			expect(await promise).toBe("cancel");
		});
	});
});
