import { beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { markTccAlertShown, tccDeniedPaths, useRepository } from "../../hooks/useRepository";
import { appLogger } from "../../stores/appLogger";
import { mockInvoke } from "../mocks/tauri";

describe("useRepository", () => {
	let repo: ReturnType<typeof useRepository>;

	beforeEach(() => {
		mockInvoke.mockReset();
		repo = useRepository();
	});

	describe("getInfo()", () => {
		it("calls invoke with path and returns repo info", async () => {
			const info = { name: "my-repo", branch: "main", dirty: false };
			mockInvoke.mockResolvedValueOnce(info);
			const result = await repo.getInfo("/repos/my-repo");
			expect(result).toEqual(info);
			expect(mockInvoke).toHaveBeenCalledWith("get_repo_info", { path: "/repos/my-repo" });
		});
	});

	describe("getDiff()", () => {
		it("calls invoke with path and returns diff string", async () => {
			const diff = "diff --git a/file.ts b/file.ts\n+added line";
			mockInvoke.mockResolvedValueOnce(diff);
			const result = await repo.getDiff("/repos/my-repo");
			expect(result).toBe(diff);
			expect(mockInvoke).toHaveBeenCalledWith("get_git_diff", { path: "/repos/my-repo" });
		});
	});

	describe("openInApp()", () => {
		it("calls invoke with path and app name", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await repo.openInApp("/repos/my-repo", "code");
			expect(mockInvoke).toHaveBeenCalledWith("open_in_app", {
				path: "/repos/my-repo",
				app: "code",
			});
		});
	});

	describe("renameBranch()", () => {
		it("calls invoke with repo path, old name, and new name", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await repo.renameBranch("/repos/my-repo", "old-branch", "new-branch");
			expect(mockInvoke).toHaveBeenCalledWith("rename_branch", {
				path: "/repos/my-repo",
				oldName: "old-branch",
				newName: "new-branch",
			});
		});
	});

	describe("getDiffStats()", () => {
		it("returns stats on success", async () => {
			const stats = { additions: 42, deletions: 7 };
			mockInvoke.mockResolvedValueOnce(stats);
			const result = await repo.getDiffStats("/repos/my-repo");
			expect(result).toEqual(stats);
			expect(mockInvoke).toHaveBeenCalledWith("get_diff_stats", { path: "/repos/my-repo" });
		});

		it("returns zeroed stats on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("not a git repo"));
			const result = await repo.getDiffStats("/repos/my-repo");
			expect(result).toEqual({ additions: 0, deletions: 0 });
		});
	});

	describe("removeWorktree()", () => {
		// Addressed by workspace id, not branch: a minted id proves the value is
		// forwarded opaquely rather than re-derived from a branch (#726-5ac7).
		it("calls invoke with repo path, workspace id, and deleteBranch", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await repo.removeWorktree("/repos/my-repo", "feature-x~a1b2c3d4", true);
			expect(mockInvoke).toHaveBeenCalledWith("remove_worktree", {
				repoPath: "/repos/my-repo",
				workspaceId: "feature-x~a1b2c3d4",
				deleteBranch: true,
				force: false,
				overrideBusy: false,
			});
		});

		it("passes deleteBranch=false when requested", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await repo.removeWorktree("/repos/my-repo", "feature-x~a1b2c3d4", false);
			expect(mockInvoke).toHaveBeenCalledWith("remove_worktree", {
				repoPath: "/repos/my-repo",
				workspaceId: "feature-x~a1b2c3d4",
				deleteBranch: false,
				force: false,
				overrideBusy: false,
			});
		});

		it("passes overrideBusy through when the busy-liveness gate is being overridden", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await repo.removeWorktree("/repos/my-repo", "feature-x", true, false, true);
			expect(mockInvoke).toHaveBeenCalledWith("remove_worktree", {
				repoPath: "/repos/my-repo",
				workspaceId: "feature-x",
				deleteBranch: true,
				force: false,
				overrideBusy: true,
			});
		});
	});

	describe("createWorktree()", () => {
		it("calls invoke with base repo and branch, returns result", async () => {
			const expected = {
				name: "feature-y",
				path: "/worktrees/feature-y",
				branch: "feature-y",
				base_repo: "/repos/my-repo",
			};
			mockInvoke.mockResolvedValueOnce(expected);
			const result = await repo.createWorktree("/repos/my-repo", "feature-y");
			expect(result).toEqual(expected);
			expect(mockInvoke).toHaveBeenCalledWith("create_worktree", {
				baseRepo: "/repos/my-repo",
				branchName: "feature-y",
			});
		});
	});

	describe("getWorktreePaths()", () => {
		// Keyed by workspace id with the branch as a field, so two workspaces on one
		// branch stay distinct instead of collapsing (#726-5ac7).
		it("returns the workspace records on success", async () => {
			const paths = {
				"feature-a": { branch: "feature-a", path: "/wt/feature-a" },
				"feature-a~a1b2c3d4": { branch: "feature-a", path: "/clones/feature-a-2" },
			};
			mockInvoke.mockResolvedValueOnce(paths);
			const result = await repo.getWorktreePaths("/repos/my-repo");
			expect(result).toEqual(paths);
			expect(mockInvoke).toHaveBeenCalledWith("get_worktree_paths", {
				repoPath: "/repos/my-repo",
			});
		});

		it("returns empty object on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("git error"));
			const result = await repo.getWorktreePaths("/repos/my-repo");
			expect(result).toEqual({});
		});
	});

	describe("listReviewSessions()", () => {
		it("calls invoke with camelCase args and returns the list", async () => {
			const sessions = [{ session_id: "s1" }];
			mockInvoke.mockResolvedValueOnce(sessions);
			const result = await repo.listReviewSessions("/repos/my-repo", 10, true);
			expect(result).toEqual(sessions);
			expect(mockInvoke).toHaveBeenCalledWith("list_review_sessions", {
				repoPath: "/repos/my-repo",
				limit: 10,
				includeCounts: true,
			});
		});

		it("swallows an error and returns an empty array", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			const result = await repo.listReviewSessions("/repos/my-repo");
			expect(result).toEqual([]);
		});
	});

	describe("getSessionReview()", () => {
		it("calls invoke with camelCase args and returns the review", async () => {
			const review = { session_id: "s1", steps: [], files: [] };
			mockInvoke.mockResolvedValueOnce(review);
			const result = await repo.getSessionReview("/repos/my-repo", "s1", false);
			expect(result).toEqual(review);
			expect(mockInvoke).toHaveBeenCalledWith("get_session_review", {
				repoPath: "/repos/my-repo",
				sessionId: "s1",
				includeSubagents: false,
			});
		});

		it("rethrows on error rather than returning an empty review", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			await expect(repo.getSessionReview("/repos/my-repo", "s1")).rejects.toThrow("boom");
		});
	});

	describe("revertSessionStep()", () => {
		it("calls invoke with tool_use_id, not a step index, and rethrows on error", async () => {
			const result = { applied: true, method: "git_apply_reverse", abs_path: "/f.ts", message: null };
			mockInvoke.mockResolvedValueOnce(result);
			const got = await repo.revertSessionStep("/repos/my-repo", "s1", "toolu_abc", true);
			expect(got).toEqual(result);
			expect(mockInvoke).toHaveBeenCalledWith("revert_session_step", {
				repoPath: "/repos/my-repo",
				sessionId: "s1",
				toolUseId: "toolu_abc",
				dryRun: true,
			});

			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			await expect(repo.revertSessionStep("/repos/my-repo", "s1", "toolu_abc")).rejects.toThrow("boom");
		});
	});

	describe("revertFileToSessionStart()", () => {
		it("calls invoke with absPath/force/dryRun and rethrows on error", async () => {
			const result = { applied: true, method: "restore_backup", abs_path: "/f.ts", message: null };
			mockInvoke.mockResolvedValueOnce(result);
			const got = await repo.revertFileToSessionStart("/repos/my-repo", "s1", "/f.ts", true, false);
			expect(got).toEqual(result);
			expect(mockInvoke).toHaveBeenCalledWith("revert_file_to_session_start", {
				repoPath: "/repos/my-repo",
				sessionId: "s1",
				absPath: "/f.ts",
				force: true,
				dryRun: false,
			});

			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			await expect(repo.revertFileToSessionStart("/repos/my-repo", "s1", "/f.ts")).rejects.toThrow("boom");
		});
	});

	describe("createBranch()", () => {
		it("calls invoke with path, name, startPoint, and checkout", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await repo.createBranch("/repos/my-repo", "feature", "main", true);
			expect(mockInvoke).toHaveBeenCalledWith("create_branch", {
				path: "/repos/my-repo",
				name: "feature",
				startPoint: "main",
				checkout: true,
			});
		});

		it("rethrows on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("branch exists"));
			await expect(repo.createBranch("/repos/my-repo", "feature", null, false)).rejects.toThrow("branch exists");
		});
	});

	describe("checkWorktreeDirty()", () => {
		it("returns the dirty flag on success", async () => {
			mockInvoke.mockResolvedValueOnce(true);
			const result = await repo.checkWorktreeDirty("/repos/my-repo", "feature");
			expect(result).toBe(true);
			expect(mockInvoke).toHaveBeenCalledWith("check_worktree_dirty", {
				repoPath: "/repos/my-repo",
				branchName: "feature",
			});
		});

		it("returns null (not false) when the backend can't answer", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("git failure"));
			const result = await repo.checkWorktreeDirty("/repos/my-repo", "feature");
			expect(result).toBeNull();
		});
	});

	describe("getWorkspaceLifecycle()", () => {
		it("maps snake_case fields to the camelCase WorkspaceLifecycleStatus shape", async () => {
			mockInvoke.mockResolvedValueOnce({
				dirty: false,
				commit_status: "merged",
				removal_safety: "safe",
				error: undefined,
			});
			const result = await repo.getWorkspaceLifecycle("/repos/my-repo", "ws-1");
			expect(result).toEqual({ dirty: false, commitStatus: "merged", removalSafety: "safe", error: undefined });
			expect(mockInvoke).toHaveBeenCalledWith("get_workspace_lifecycle", {
				repoPath: "/repos/my-repo",
				workspaceId: "ws-1",
			});
		});
	});

	describe("getChangedFiles()", () => {
		it("returns changed files on success", async () => {
			const files = [{ path: "a.ts", status: "M", additions: 1, deletions: 0 }];
			mockInvoke.mockResolvedValueOnce(files);
			const result = await repo.getChangedFiles("/repos/my-repo", "main");
			expect(result).toEqual(files);
			expect(mockInvoke).toHaveBeenCalledWith("get_changed_files", { path: "/repos/my-repo", scope: "main" });
		});

		it("returns an empty array on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			const result = await repo.getChangedFiles("/repos/my-repo");
			expect(result).toEqual([]);
		});
	});

	describe("getFileDiff()", () => {
		it("returns the diff on success", async () => {
			mockInvoke.mockResolvedValueOnce("diff content");
			const result = await repo.getFileDiff("/repos/my-repo", "a.ts", "main", true);
			expect(result).toBe("diff content");
			expect(mockInvoke).toHaveBeenCalledWith("get_file_diff", {
				path: "/repos/my-repo",
				file: "a.ts",
				scope: "main",
				untracked: true,
			});
		});

		it("returns an empty string on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			const result = await repo.getFileDiff("/repos/my-repo", "a.ts");
			expect(result).toBe("");
		});
	});

	describe("listMarkdownFiles()", () => {
		it("returns entries on success", async () => {
			const entries = [{ path: "README.md", git_status: "", is_ignored: false, modified_at: 0 }];
			mockInvoke.mockResolvedValueOnce(entries);
			const result = await repo.listMarkdownFiles("/repos/my-repo");
			expect(result).toEqual(entries);
		});

		it("returns an empty array on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			const result = await repo.listMarkdownFiles("/repos/my-repo");
			expect(result).toEqual([]);
		});
	});

	describe("readFile()", () => {
		it("returns file content on success", async () => {
			mockInvoke.mockResolvedValueOnce("file body");
			const result = await repo.readFile("/repos/my-repo", "README.md");
			expect(result).toBe("file body");
			expect(mockInvoke).toHaveBeenCalledWith("read_file", { path: "/repos/my-repo", file: "README.md" });
		});

		it("returns an empty string and logs at debug for a missing file (ENOENT)", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("No such file or directory (os error 2)"));
			const result = await repo.readFile("/repos/my-repo", "gone.md");
			expect(result).toBe("");
		});

		it("returns an empty string for a non-ENOENT read failure", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("permission denied"));
			const result = await repo.readFile("/repos/my-repo", "secret.md");
			expect(result).toBe("");
		});
	});

	describe("generateWorktreeName()", () => {
		it("calls invoke with the existing name list", async () => {
			mockInvoke.mockResolvedValueOnce("brave-otter");
			const result = await repo.generateWorktreeName(["taken-1"]);
			expect(result).toBe("brave-otter");
			expect(mockInvoke).toHaveBeenCalledWith("generate_worktree_name_cmd", { existingNames: ["taken-1"] });
		});
	});

	describe("generateCloneBranchName()", () => {
		it("calls invoke with source branch and existing names", async () => {
			mockInvoke.mockResolvedValueOnce("feature--brave-otter");
			const result = await repo.generateCloneBranchName("feature", ["taken-1"]);
			expect(result).toBe("feature--brave-otter");
			expect(mockInvoke).toHaveBeenCalledWith("generate_clone_branch_name_cmd", {
				sourceBranch: "feature",
				existingNames: ["taken-1"],
			});
		});
	});

	describe("listBaseRefOptions()", () => {
		it("returns options on success", async () => {
			const options = [{ name: "main", kind: "local", is_default: true }];
			mockInvoke.mockResolvedValueOnce(options);
			const result = await repo.listBaseRefOptions("/repos/my-repo");
			expect(result).toEqual(options);
		});

		it("returns an empty array on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			const result = await repo.listBaseRefOptions("/repos/my-repo");
			expect(result).toEqual([]);
		});
	});

	describe("mergeAndArchiveWorktree()", () => {
		it("calls invoke with all fields, defaulting force to false", async () => {
			const result = {
				merged: true,
				action: "archived",
				archive_path: "/archive/feature",
				commits_ahead: 3,
				worktree_dirty: false,
			};
			mockInvoke.mockResolvedValueOnce(result);
			const got = await repo.mergeAndArchiveWorktree("/repos/my-repo", "feature", "ws-1", "main", "archive");
			expect(got).toEqual(result);
			expect(mockInvoke).toHaveBeenCalledWith("merge_and_archive_worktree", {
				repoPath: "/repos/my-repo",
				branchName: "feature",
				workspaceId: "ws-1",
				targetBranch: "main",
				afterMerge: "archive",
				force: false,
			});
		});
	});

	describe("finalizeMergedWorktree()", () => {
		it("calls invoke with action and force", async () => {
			const result = {
				merged: true,
				action: "deleted",
				archive_path: null,
				commits_ahead: 0,
				worktree_dirty: false,
			};
			mockInvoke.mockResolvedValueOnce(result);
			const got = await repo.finalizeMergedWorktree("/repos/my-repo", "ws-1", "delete", true);
			expect(got).toEqual(result);
			expect(mockInvoke).toHaveBeenCalledWith("finalize_merged_worktree", {
				repoPath: "/repos/my-repo",
				workspaceId: "ws-1",
				action: "delete",
				force: true,
			});
		});
	});

	describe("getMergedBranches()", () => {
		it("returns branch names on success", async () => {
			mockInvoke.mockResolvedValueOnce(["old-feature"]);
			const result = await repo.getMergedBranches("/repos/my-repo");
			expect(result).toEqual(["old-feature"]);
			expect(mockInvoke).toHaveBeenCalledWith("get_merged_branches", { path: "/repos/my-repo" });
		});

		it("returns an empty array on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			const result = await repo.getMergedBranches("/repos/my-repo");
			expect(result).toEqual([]);
		});
	});

	describe("getRepoSummary()", () => {
		it("returns the aggregate snapshot on success", async () => {
			const summary = {
				worktree_paths: {},
				merged_branches: [],
				diff_stats: {},
				last_commit_ts: {},
				workspace_statuses: {},
			};
			mockInvoke.mockResolvedValueOnce(summary);
			const result = await repo.getRepoSummary("/repos/my-repo");
			expect(result).toEqual(summary);
		});

		it("returns a zeroed snapshot on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			const result = await repo.getRepoSummary("/repos/my-repo");
			expect(result).toEqual({
				worktree_paths: {},
				merged_branches: [],
				diff_stats: {},
				last_commit_ts: {},
				workspace_statuses: {},
			});
		});
	});

	describe("switchBranch()", () => {
		it("defaults force/stash to false when opts is omitted", async () => {
			const result = { success: true, stashed: false, previous_branch: "main", new_branch: "feature" };
			mockInvoke.mockResolvedValueOnce(result);
			const got = await repo.switchBranch("/repos/my-repo", "feature");
			expect(got).toEqual(result);
			expect(mockInvoke).toHaveBeenCalledWith("switch_branch", {
				repoPath: "/repos/my-repo",
				branchName: "feature",
				force: false,
				stash: false,
			});
		});

		it("forwards force/stash opts through", async () => {
			mockInvoke.mockResolvedValueOnce({
				success: true,
				stashed: true,
				previous_branch: "main",
				new_branch: "feature",
			});
			await repo.switchBranch("/repos/my-repo", "feature", { force: true, stash: true });
			expect(mockInvoke).toHaveBeenCalledWith("switch_branch", {
				repoPath: "/repos/my-repo",
				branchName: "feature",
				force: true,
				stash: true,
			});
		});

		it("rethrows on error (e.g. a dirty working tree)", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("dirty"));
			await expect(repo.switchBranch("/repos/my-repo", "feature")).rejects.toThrow("dirty");
		});
	});

	describe("checkoutRemoteBranch()", () => {
		it("calls invoke with repoPath and branchName", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await repo.checkoutRemoteBranch("/repos/my-repo", "origin/feature");
			expect(mockInvoke).toHaveBeenCalledWith("checkout_remote_branch", {
				repoPath: "/repos/my-repo",
				branchName: "origin/feature",
			});
		});

		it("rethrows on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			await expect(repo.checkoutRemoteBranch("/repos/my-repo", "origin/feature")).rejects.toThrow("boom");
		});
	});

	describe("detectOrphanWorktrees()", () => {
		it("returns orphan paths on success", async () => {
			mockInvoke.mockResolvedValueOnce(["/repos/my-repo/.worktrees/orphan"]);
			const result = await repo.detectOrphanWorktrees("/repos/my-repo");
			expect(result).toEqual(["/repos/my-repo/.worktrees/orphan"]);
		});

		it("returns an empty array on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			const result = await repo.detectOrphanWorktrees("/repos/my-repo");
			expect(result).toEqual([]);
		});
	});

	describe("removeOrphanWorktree()", () => {
		it("returns the archive destination path", async () => {
			mockInvoke.mockResolvedValueOnce("/archive/orphan");
			const result = await repo.removeOrphanWorktree("/repos/my-repo", "/repos/my-repo/.worktrees/orphan");
			expect(result).toBe("/archive/orphan");
			expect(mockInvoke).toHaveBeenCalledWith("remove_orphan_worktree", {
				repoPath: "/repos/my-repo",
				worktreePath: "/repos/my-repo/.worktrees/orphan",
			});
		});

		it("rethrows on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			await expect(repo.removeOrphanWorktree("/repos/my-repo", "/repos/my-repo/.worktrees/orphan")).rejects.toThrow(
				"boom",
			);
		});
	});

	describe("deleteOrphanWorktree()", () => {
		it("calls invoke with repoPath and worktreePath", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await repo.deleteOrphanWorktree("/repos/my-repo", "/repos/my-repo/.worktrees/orphan");
			expect(mockInvoke).toHaveBeenCalledWith("delete_orphan_worktree", {
				repoPath: "/repos/my-repo",
				worktreePath: "/repos/my-repo/.worktrees/orphan",
			});
		});

		it("rethrows on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			await expect(repo.deleteOrphanWorktree("/repos/my-repo", "/repos/my-repo/.worktrees/orphan")).rejects.toThrow(
				"boom",
			);
		});
	});

	describe("mergePrViaGithub()", () => {
		it("calls invoke with repoPath, prNumber, and mergeMethod", async () => {
			mockInvoke.mockResolvedValueOnce("merged sha abc123");
			const result = await repo.mergePrViaGithub("/repos/my-repo", 42, "squash");
			expect(result).toBe("merged sha abc123");
			expect(mockInvoke).toHaveBeenCalledWith("merge_pr_via_github", {
				repoPath: "/repos/my-repo",
				prNumber: 42,
				mergeMethod: "squash",
			});
		});

		it("rethrows on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			await expect(repo.mergePrViaGithub("/repos/my-repo", 42, "merge")).rejects.toThrow("boom");
		});
	});

	describe("runSetupScript()", () => {
		it("calls invoke with script and cwd, returns exit info", async () => {
			const result = { exit_code: 0, stdout: "ok", stderr: "" };
			mockInvoke.mockResolvedValueOnce(result);
			const got = await repo.runSetupScript("npm install", "/repos/my-repo");
			expect(got).toEqual(result);
			expect(mockInvoke).toHaveBeenCalledWith("run_setup_script", { script: "npm install", cwd: "/repos/my-repo" });
		});

		it("rethrows on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			await expect(repo.runSetupScript("npm install", "/repos/my-repo")).rejects.toThrow("boom");
		});
	});

	describe("getRecentCommits()", () => {
		it("returns commits on success", async () => {
			const commits = [{ hash: "abc123", short_hash: "abc123", subject: "fix bug" }];
			mockInvoke.mockResolvedValueOnce(commits);
			const result = await repo.getRecentCommits("/repos/my-repo", 5);
			expect(result).toEqual(commits);
			expect(mockInvoke).toHaveBeenCalledWith("get_recent_commits", { path: "/repos/my-repo", count: 5 });
		});

		it("returns an empty array on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			const result = await repo.getRecentCommits("/repos/my-repo");
			expect(result).toEqual([]);
		});
	});

	// `tccDeniedPaths`/`tccAlertShown` are module-level singletons by design — "global,
	// shown once per session" (see useRepository.ts's own comment) — not per-hook-instance
	// state reset by the `beforeEach` above. These tests use distinct repo paths so they
	// don't depend on each other's ordering, EXCEPT the last one, which relies on running
	// after the alert has never been marked shown by anything earlier in this file; it must
	// stay last in this describe block.
	describe("TCC (macOS permission) detection", () => {
		it("records the repo path when git reports a TCC 'Operation not permitted' denial", async () => {
			// The exact shape observed in the wild: git can't even read its cwd.
			mockInvoke.mockRejectedValueOnce(
				new Error(
					"git branch failed: git exited with code 128: fatal: Unable to read current working directory: Operation not permitted",
				),
			);
			const result = await repo.listLocalBranches("/repos/tcc-denied-a");
			expect(result).toEqual([]);
			expect(tccDeniedPaths()).toContain("/repos/tcc-denied-a");
		});

		it("does not record the repo path for an unrelated git error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("fatal: not a git repository"));
			const result = await repo.listLocalBranches("/repos/tcc-denied-b");
			expect(result).toEqual([]);
			expect(tccDeniedPaths()).not.toContain("/repos/tcc-denied-b");
		});

		it("dedupes repeated denials for the same path", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("Operation not permitted"));
			await repo.listLocalBranches("/repos/tcc-denied-a");
			mockInvoke.mockRejectedValueOnce(new Error("Operation not permitted"));
			await repo.listLocalBranches("/repos/tcc-denied-a");
			const occurrences = tccDeniedPaths().filter((path) => path === "/repos/tcc-denied-a").length;
			expect(occurrences).toBe(1);
		});

		it("detects a denial even when the rejection value is not an Error instance", async () => {
			// A Tauri command can reject with a raw string rather than an Error; the
			// message must still be checked via String(err), not err.message.
			mockInvoke.mockRejectedValueOnce("Operation not permitted");
			await repo.listLocalBranches("/repos/tcc-denied-non-error");
			expect(tccDeniedPaths()).toContain("/repos/tcc-denied-non-error");
		});

		it("logs a TCC denial once per path, then suppresses repeats on every subsequent retry", async () => {
			// The refresh loop retries listLocalBranches on every debounced
			// repo-changed event, so a permanently-denied repo must not spam the
			// log forever — only the first occurrence should be logged.
			const errorSpy = vi.spyOn(appLogger, "error").mockImplementation(() => {});
			try {
				mockInvoke.mockRejectedValueOnce(new Error("Operation not permitted"));
				await repo.listLocalBranches("/repos/tcc-denied-log-spam");
				mockInvoke.mockRejectedValueOnce(new Error("Operation not permitted"));
				await repo.listLocalBranches("/repos/tcc-denied-log-spam");
				mockInvoke.mockRejectedValueOnce(new Error("Operation not permitted"));
				await repo.listLocalBranches("/repos/tcc-denied-log-spam");
				expect(errorSpy).toHaveBeenCalledTimes(1);
			} finally {
				errorSpy.mockRestore();
			}
		});

		it("does not suppress repeated logging for a non-TCC failure on the same path", async () => {
			const errorSpy = vi.spyOn(appLogger, "error").mockImplementation(() => {});
			try {
				mockInvoke.mockRejectedValueOnce(new Error("fatal: not a git repository"));
				await repo.listLocalBranches("/repos/tcc-denied-unrelated-repeat");
				mockInvoke.mockRejectedValueOnce(new Error("fatal: not a git repository"));
				await repo.listLocalBranches("/repos/tcc-denied-unrelated-repeat");
				expect(errorSpy).toHaveBeenCalledTimes(2);
			} finally {
				errorSpy.mockRestore();
			}
		});

		it("detects the same denial through getRepoStructure() and getRepoDiffStats()", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("Operation not permitted"));
			await repo.getRepoStructure("/repos/tcc-denied-c");
			expect(tccDeniedPaths()).toContain("/repos/tcc-denied-c");

			mockInvoke.mockRejectedValueOnce(new Error("Operation not permitted"));
			await repo.getRepoDiffStats("/repos/tcc-denied-d");
			expect(tccDeniedPaths()).toContain("/repos/tcc-denied-d");
		});

		it("stops recording new denials once the alert has already been shown — must run last", async () => {
			markTccAlertShown();
			mockInvoke.mockRejectedValueOnce(new Error("Operation not permitted"));
			await repo.listLocalBranches("/repos/tcc-denied-after-alert");
			expect(tccDeniedPaths()).not.toContain("/repos/tcc-denied-after-alert");
		});
	});
});
