import { beforeEach, describe, expect, it } from "vitest";
import "../mocks/tauri";
import { useRepository } from "../../hooks/useRepository";
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
});
