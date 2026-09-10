import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { mockInvoke } from "./mocks/tauri";

// isTauri() checks __TAURI_INTERNALS__ — set to ensure `initDeepLinkHandler`
// (which bails in a non-Tauri context) would run. `handleDeepLink` is
// exported separately and doesn't need this, but the module-level guard on
// `transport.isTauri()` uses it elsewhere.
(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};

import { handleDeepLink } from "../deep-link-handler";
import { repositoriesStore } from "../stores/repositories";

const setActiveSpy = vi.spyOn(repositoriesStore, "setActive").mockImplementation(() => {});

const callbacks = {
	openSettings: vi.fn(),
	confirm: vi.fn().mockResolvedValue(true),
	onInstallError: vi.fn(),
	openRepoPath: vi.fn().mockResolvedValue(undefined),
	chooseRepoForPath: vi.fn().mockResolvedValue(null),
	handleAddTerminalToBranch: vi.fn().mockResolvedValue("term-1"),
	openUnattachedTerminal: vi.fn(),
	markTerminalPlacementAsGuess: vi.fn(),
};

// Mutating the repositories store schedules its debounced save; without this the
// timer outlives the run and vitest reports an async leak.
afterEach(() => repositoriesStore._testCancelPendingSave());

describe("deep link handler — OAuth callback", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
		callbacks.openSettings.mockReset();
		callbacks.confirm.mockReset().mockResolvedValue(true);
		callbacks.onInstallError.mockReset();
	});

	it("invokes mcp_oauth_callback with code + state from tuic://oauth-callback", async () => {
		await handleDeepLink("tuic://oauth-callback?code=AUTH_CODE&state=NONCE_123", callbacks);

		expect(mockInvoke).toHaveBeenCalledTimes(1);
		expect(mockInvoke).toHaveBeenCalledWith("mcp_oauth_callback", {
			code: "AUTH_CODE",
			oauthState: "NONCE_123",
		});
		expect(callbacks.onInstallError).not.toHaveBeenCalled();
	});

	it("surfaces invoke failures via onInstallError", async () => {
		mockInvoke.mockRejectedValueOnce("token exchange failed");

		await handleDeepLink("tuic://oauth-callback?code=AUTH_CODE&state=NONCE", callbacks);

		expect(callbacks.onInstallError).toHaveBeenCalledWith(expect.stringContaining("token exchange failed"));
	});

	it("skips invoke when code is missing", async () => {
		await handleDeepLink("tuic://oauth-callback?state=NONCE", callbacks);
		expect(mockInvoke).not.toHaveBeenCalled();
	});

	it("skips invoke when state is missing", async () => {
		await handleDeepLink("tuic://oauth-callback?code=XYZ", callbacks);
		expect(mockInvoke).not.toHaveBeenCalled();
	});

	it("handles authorization server error responses", async () => {
		await handleDeepLink("tuic://oauth-callback?error=access_denied&error_description=user%20cancelled", callbacks);

		expect(mockInvoke).not.toHaveBeenCalled();
		expect(callbacks.onInstallError).toHaveBeenCalledWith(expect.stringContaining("access_denied"));
	});
});

describe("deep link handler — open-repo (`tuic <dir>`)", () => {
	beforeEach(() => {
		callbacks.confirm.mockReset().mockResolvedValue(true);
		callbacks.openRepoPath.mockReset().mockResolvedValue(undefined);
		setActiveSpy.mockClear();
	});

	it("activates a repo that is already in the list, without asking", async () => {
		repositoriesStore.add({ path: "/src/known", displayName: "known" });

		await handleDeepLink("tuic://open-repo?path=/src/known", callbacks);

		expect(setActiveSpy).toHaveBeenCalledWith("/src/known");
		expect(callbacks.confirm).not.toHaveBeenCalled();
		expect(callbacks.openRepoPath).not.toHaveBeenCalled();
	});

	it("adds an unknown repo after confirmation — this is what `tuic .` does in a new project", async () => {
		await handleDeepLink("tuic://open-repo?path=/src/new-project", callbacks);

		expect(callbacks.confirm).toHaveBeenCalledTimes(1);
		expect(callbacks.openRepoPath).toHaveBeenCalledWith("/src/new-project");
	});

	it("adds nothing when the confirmation is denied", async () => {
		callbacks.confirm.mockResolvedValue(false);

		await handleDeepLink("tuic://open-repo?path=/src/new-project", callbacks);

		expect(callbacks.openRepoPath).not.toHaveBeenCalled();
		expect(setActiveSpy).not.toHaveBeenCalled();
	});

	it("ignores a link with no path", async () => {
		await handleDeepLink("tuic://open-repo", callbacks);

		expect(callbacks.confirm).not.toHaveBeenCalled();
		expect(callbacks.openRepoPath).not.toHaveBeenCalled();
	});
});

describe('deep link handler — open-terminal (Finder "Open Here")', () => {
	beforeEach(() => {
		callbacks.openRepoPath.mockReset().mockResolvedValue(undefined);
		callbacks.chooseRepoForPath.mockReset().mockResolvedValue(null);
		callbacks.handleAddTerminalToBranch.mockReset().mockResolvedValue("term-1");
		callbacks.openUnattachedTerminal.mockReset();
		callbacks.markTerminalPlacementAsGuess.mockReset();
		// Unlike the open-repo tests above (which only observe setActive calls),
		// these tests need the real "active repo" fallback rung of the placement
		// ladder to actually see a changed activeRepoPath — restore the real
		// implementation for this block. No later describe block in this file
		// depends on setActiveSpy still intercepting calls.
		setActiveSpy.mockRestore();
	});

	afterEach(() => {
		for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
	});

	it("opens directly in the owning repo/branch when the path is inside a linked worktree — no picker", async () => {
		repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
		repositoriesStore.setBranch("/Gits/alpha", "feature", { worktreePath: "/Gits/alpha__wt/feature" });

		await handleDeepLink("tuic://open-terminal?path=/Gits/alpha__wt/feature/src", callbacks);

		expect(callbacks.handleAddTerminalToBranch).toHaveBeenCalledWith(
			"/Gits/alpha",
			"feature",
			"/Gits/alpha__wt/feature/src",
		);
		expect(callbacks.chooseRepoForPath).not.toHaveBeenCalled();
		// A real (non-guessed) placement must keep its recorded ownership —
		// handleAddTerminalToBranch already set it correctly.
		expect(callbacks.markTerminalPlacementAsGuess).not.toHaveBeenCalled();
	});

	it("uses the exact clicked folder as cwd even when it's deep inside the repo root", async () => {
		repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
		repositoriesStore.setBranch("/Gits/alpha", "main", { worktreePath: "/Gits/alpha" });
		repositoriesStore.setActiveBranch("/Gits/alpha", "main");

		await handleDeepLink("tuic://open-terminal?path=/Gits/alpha/packages/app", callbacks);

		expect(callbacks.handleAddTerminalToBranch).toHaveBeenCalledWith("/Gits/alpha", "main", "/Gits/alpha/packages/app");
	});

	it("falls back to the active repo when nothing owns the path — no picker", async () => {
		repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
		repositoriesStore.setBranch("/Gits/alpha", "main", { worktreePath: "/Gits/alpha" });
		repositoriesStore.setActiveBranch("/Gits/alpha", "main");
		repositoriesStore.setActive("/Gits/alpha");

		await handleDeepLink("tuic://open-terminal?path=/elsewhere/unrelated", callbacks);

		expect(callbacks.handleAddTerminalToBranch).toHaveBeenCalledWith("/Gits/alpha", "main", "/elsewhere/unrelated");
		expect(callbacks.chooseRepoForPath).not.toHaveBeenCalled();
	});

	it("clears the recorded owner for a guessed (active-repo-fallback) placement — nothing actually claims this cwd", async () => {
		repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
		repositoriesStore.setBranch("/Gits/alpha", "main", { worktreePath: "/Gits/alpha" });
		repositoriesStore.setActiveBranch("/Gits/alpha", "main");
		repositoriesStore.setActive("/Gits/alpha");
		callbacks.handleAddTerminalToBranch.mockResolvedValue("term-guessed");

		await handleDeepLink("tuic://open-terminal?path=/elsewhere/unrelated", callbacks);

		expect(callbacks.markTerminalPlacementAsGuess).toHaveBeenCalledWith("term-guessed");
	});

	it("does not try to clear ownership when handleAddTerminalToBranch returns no id", async () => {
		repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
		repositoriesStore.setBranch("/Gits/alpha", "main", { worktreePath: "/Gits/alpha" });
		repositoriesStore.setActiveBranch("/Gits/alpha", "main");
		repositoriesStore.setActive("/Gits/alpha");
		callbacks.handleAddTerminalToBranch.mockResolvedValue(undefined);

		await handleDeepLink("tuic://open-terminal?path=/elsewhere/unrelated", callbacks);

		expect(callbacks.markTerminalPlacementAsGuess).not.toHaveBeenCalled();
	});

	it("asks the user when nothing owns the path and there is no active repo", async () => {
		await handleDeepLink("tuic://open-terminal?path=/elsewhere/unrelated", callbacks);

		expect(callbacks.chooseRepoForPath).toHaveBeenCalledWith("/elsewhere/unrelated");
		expect(callbacks.handleAddTerminalToBranch).not.toHaveBeenCalled();
	});

	it("picker choice: an existing repo resolves its branch and attaches the terminal", async () => {
		repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
		repositoriesStore.setBranch("/Gits/beta", "main", { worktreePath: "/Gits/beta" });
		repositoriesStore.setActiveBranch("/Gits/beta", "main");
		callbacks.chooseRepoForPath.mockResolvedValue({ kind: "repo", repoPath: "/Gits/beta" });

		await handleDeepLink("tuic://open-terminal?path=/elsewhere/unrelated", callbacks);

		expect(callbacks.handleAddTerminalToBranch).toHaveBeenCalledWith("/Gits/beta", "main", "/elsewhere/unrelated");
	});

	it("picker choice: an existing repo with no resolvable branch opens unattached instead", async () => {
		// Registered but no branch recorded at all — the defensive edge case.
		repositoriesStore.add({ path: "/Gits/empty", displayName: "empty" });
		callbacks.chooseRepoForPath.mockResolvedValue({ kind: "repo", repoPath: "/Gits/empty" });

		await handleDeepLink("tuic://open-terminal?path=/elsewhere/unrelated", callbacks);

		expect(callbacks.handleAddTerminalToBranch).not.toHaveBeenCalled();
		expect(callbacks.openUnattachedTerminal).toHaveBeenCalledWith("/elsewhere/unrelated");
	});

	it("picker choice: register registers the exact clicked path as a new repo", async () => {
		callbacks.chooseRepoForPath.mockResolvedValue({ kind: "register" });

		await handleDeepLink("tuic://open-terminal?path=/elsewhere/new-project", callbacks);

		expect(callbacks.openRepoPath).toHaveBeenCalledWith("/elsewhere/new-project");
		expect(callbacks.handleAddTerminalToBranch).not.toHaveBeenCalled();
	});

	it("picker choice: unattached opens a terminal with no repo association", async () => {
		callbacks.chooseRepoForPath.mockResolvedValue({ kind: "unattached" });

		await handleDeepLink("tuic://open-terminal?path=/elsewhere/unrelated", callbacks);

		expect(callbacks.openUnattachedTerminal).toHaveBeenCalledWith("/elsewhere/unrelated");
	});

	it("picker cancel (null) does nothing further", async () => {
		callbacks.chooseRepoForPath.mockResolvedValue(null);

		await handleDeepLink("tuic://open-terminal?path=/elsewhere/unrelated", callbacks);

		expect(callbacks.handleAddTerminalToBranch).not.toHaveBeenCalled();
		expect(callbacks.openRepoPath).not.toHaveBeenCalled();
		expect(callbacks.openUnattachedTerminal).not.toHaveBeenCalled();
	});

	it("opens one pane per path for a multi-selection", async () => {
		repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
		repositoriesStore.setBranch("/Gits/alpha", "main", { worktreePath: "/Gits/alpha" });
		repositoriesStore.setActiveBranch("/Gits/alpha", "main");
		repositoriesStore.setActive("/Gits/alpha");

		await handleDeepLink("tuic://open-terminal?path=/a&path=/b&path=/c", callbacks);

		expect(callbacks.handleAddTerminalToBranch).toHaveBeenCalledTimes(3);
		expect(callbacks.handleAddTerminalToBranch).toHaveBeenNthCalledWith(1, "/Gits/alpha", "main", "/a");
		expect(callbacks.handleAddTerminalToBranch).toHaveBeenNthCalledWith(2, "/Gits/alpha", "main", "/b");
		expect(callbacks.handleAddTerminalToBranch).toHaveBeenNthCalledWith(3, "/Gits/alpha", "main", "/c");
	});

	it("caps a selection larger than 5 paths at the first 5", async () => {
		repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
		repositoriesStore.setBranch("/Gits/alpha", "main", { worktreePath: "/Gits/alpha" });
		repositoriesStore.setActiveBranch("/Gits/alpha", "main");
		repositoriesStore.setActive("/Gits/alpha");

		const query = Array.from({ length: 7 }, (_, i) => `path=/p${i}`).join("&");
		await handleDeepLink(`tuic://open-terminal?${query}`, callbacks);

		expect(callbacks.handleAddTerminalToBranch).toHaveBeenCalledTimes(5);
		expect(callbacks.handleAddTerminalToBranch).toHaveBeenNthCalledWith(5, "/Gits/alpha", "main", "/p4");
	});

	it("does not truncate a selection of exactly 5 paths (boundary)", async () => {
		repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
		repositoriesStore.setBranch("/Gits/alpha", "main", { worktreePath: "/Gits/alpha" });
		repositoriesStore.setActiveBranch("/Gits/alpha", "main");
		repositoriesStore.setActive("/Gits/alpha");

		const query = Array.from({ length: 5 }, (_, i) => `path=/p${i}`).join("&");
		await handleDeepLink(`tuic://open-terminal?${query}`, callbacks);

		expect(callbacks.handleAddTerminalToBranch).toHaveBeenCalledTimes(5);
		expect(callbacks.handleAddTerminalToBranch).toHaveBeenNthCalledWith(5, "/Gits/alpha", "main", "/p4");
	});

	it("treats a malformed empty path value as unowned and still asks the user (no crash)", async () => {
		await handleDeepLink("tuic://open-terminal?path=", callbacks);

		expect(callbacks.chooseRepoForPath).toHaveBeenCalledWith("");
		expect(callbacks.handleAddTerminalToBranch).not.toHaveBeenCalled();
	});

	it("ignores a link with no path", async () => {
		await handleDeepLink("tuic://open-terminal", callbacks);

		expect(callbacks.chooseRepoForPath).not.toHaveBeenCalled();
		expect(callbacks.handleAddTerminalToBranch).not.toHaveBeenCalled();
	});
});

describe("deep link handler — cmd gateway (default-deny)", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
		callbacks.confirm.mockReset().mockResolvedValue(true);
		callbacks.onInstallError.mockReset();
	});

	it("runs a safe read-only command without confirmation", async () => {
		await handleDeepLink("tuic://cmd/session/list", callbacks);
		expect(callbacks.confirm).not.toHaveBeenCalled();
		expect(mockInvoke).toHaveBeenCalledWith("deep_link_mcp_call", {
			tool: "session",
			action: "list",
			params: {},
		});
	});

	it("requires confirmation for a destructive command (session/input)", async () => {
		await handleDeepLink("tuic://cmd/session/input?data=rm", callbacks);
		expect(callbacks.confirm).toHaveBeenCalledTimes(1);
		expect(mockInvoke).toHaveBeenCalledWith("deep_link_mcp_call", {
			tool: "session",
			action: "input",
			params: { data: "rm" },
		});
	});

	it("requires confirmation for an unknown command — default-deny (agent/send was previously un-gated)", async () => {
		await handleDeepLink("tuic://cmd/agent/send?to=peer", callbacks);
		expect(callbacks.confirm).toHaveBeenCalledTimes(1);
	});

	it("does not execute when confirmation is denied", async () => {
		callbacks.confirm.mockResolvedValue(false);
		await handleDeepLink("tuic://cmd/agent/send?to=peer", callbacks);
		expect(mockInvoke).not.toHaveBeenCalled();
	});

	it("never executes a blocked command, even with confirmation available", async () => {
		await handleDeepLink("tuic://cmd/config/save", callbacks);
		expect(callbacks.confirm).not.toHaveBeenCalled();
		expect(mockInvoke).not.toHaveBeenCalled();
	});
});
