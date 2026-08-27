import { beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

const { mockRepositoriesStore, mockPaneLayoutStore, mockFocus, mockTerminalsStore, mockGlobalWorkspaceStore } =
	vi.hoisted(() => {
		const mockRepositoriesStore = {
			getRepoPathForTerminal: vi.fn<(id: string) => string | null>(),
			state: {
				activeRepoPath: null as string | null,
				repositories: {} as Record<
					string,
					{ activeWorkspaceId: string | null; workspaces: Record<string, { terminals: string[] }> }
				>,
			},
			setActive: vi.fn(),
			setActiveWorkspace: vi.fn(),
			getActive: vi.fn(() => undefined),
		};

		const mockPaneLayoutStore = {
			isSplit: vi.fn(() => false),
			getGroupForTab: vi.fn<(id: string) => string | null>(() => null),
			setActiveGroup: vi.fn(),
			setActiveTab: vi.fn(),
		};

		const mockFocus = vi.fn();
		const mockTerminalsStore = {
			setActive: vi.fn(),
			get: vi.fn<(id: string) => { ref?: { focus: () => void } } | undefined>(),
		};

		// main's navigateToTerminal also routes unowned sessions through the Global
		// Workspace's manual scope; inert by default (no members, inactive).
		const mockGlobalWorkspaceStore = {
			getScopeMembers: vi.fn<(scope: string) => string[]>(() => []),
			getScope: vi.fn(() => "repo"),
			isActive: vi.fn(() => false),
			isPromoted: vi.fn(() => false),
			deactivate: vi.fn(),
			setScope: vi.fn(),
			activate: vi.fn(),
		};

		return { mockRepositoriesStore, mockPaneLayoutStore, mockFocus, mockTerminalsStore, mockGlobalWorkspaceStore };
	});

vi.mock("../../stores/repositories", () => ({ repositoriesStore: mockRepositoriesStore }));
vi.mock("../../stores/paneLayout", () => ({ paneLayoutStore: mockPaneLayoutStore }));
vi.mock("../../stores/terminals", () => ({ terminalsStore: mockTerminalsStore }));
vi.mock("../../stores/globalWorkspace", () => ({
	globalWorkspaceStore: mockGlobalWorkspaceStore,
	MANUAL_SCOPE: "manual",
}));

import { navigateToTerminal } from "../../utils/navigateToTerminal";

describe("navigateToTerminal", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockRepositoriesStore.getRepoPathForTerminal.mockReturnValue(null);
		mockRepositoriesStore.state.activeRepoPath = null;
		mockRepositoriesStore.state.repositories = {};
		mockPaneLayoutStore.isSplit.mockReturnValue(false);
		mockPaneLayoutStore.getGroupForTab.mockReturnValue(null);
		mockTerminalsStore.get.mockReturnValue({ ref: { focus: mockFocus } });
		mockGlobalWorkspaceStore.getScopeMembers.mockReturnValue([]);
		mockGlobalWorkspaceStore.getScope.mockReturnValue("repo");
		mockGlobalWorkspaceStore.isActive.mockReturnValue(false);
		vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback): number => {
			cb(0);
			return 0;
		});
	});

	it("activates the terminal", () => {
		navigateToTerminal("t1");
		expect(mockTerminalsStore.setActive).toHaveBeenCalledWith("t1");
	});

	it("focuses the terminal's ref on the next frame", () => {
		navigateToTerminal("t1");
		expect(mockFocus).toHaveBeenCalledOnce();
	});

	it("does nothing to repo/workspace state when the terminal has no owning repo", () => {
		mockRepositoriesStore.getRepoPathForTerminal.mockReturnValue(null);
		navigateToTerminal("t1");
		expect(mockRepositoriesStore.setActive).not.toHaveBeenCalled();
		expect(mockRepositoriesStore.setActiveWorkspace).not.toHaveBeenCalled();
		expect(mockTerminalsStore.setActive).toHaveBeenCalledWith("t1");
	});

	it("switches repo and branch when the terminal lives in a different repo/workspace", () => {
		mockRepositoriesStore.getRepoPathForTerminal.mockReturnValue("/repo/a");
		mockRepositoriesStore.state.activeRepoPath = "/repo/other";
		mockRepositoriesStore.state.repositories = {
			"/repo/a": { activeWorkspaceId: "ws-old", workspaces: { "ws-main": { terminals: ["t1"] } } },
		};

		navigateToTerminal("t1");

		expect(mockRepositoriesStore.setActive).toHaveBeenCalledWith("/repo/a");
		expect(mockRepositoriesStore.setActiveWorkspace).toHaveBeenCalledWith("/repo/a", "ws-main");
	});

	it("skips repo/workspace writes when already active", () => {
		mockRepositoriesStore.getRepoPathForTerminal.mockReturnValue("/repo/a");
		mockRepositoriesStore.state.activeRepoPath = "/repo/a";
		mockRepositoriesStore.state.repositories = {
			"/repo/a": { activeWorkspaceId: "ws-main", workspaces: { "ws-main": { terminals: ["t1"] } } },
		};

		navigateToTerminal("t1");

		expect(mockRepositoriesStore.setActive).not.toHaveBeenCalled();
		expect(mockRepositoriesStore.setActiveWorkspace).not.toHaveBeenCalled();
	});

	it("sets the active pane group and tab when the layout is split", () => {
		mockPaneLayoutStore.isSplit.mockReturnValue(true);
		mockPaneLayoutStore.getGroupForTab.mockReturnValue("group-1");

		navigateToTerminal("t1");

		expect(mockPaneLayoutStore.setActiveGroup).toHaveBeenCalledWith("group-1");
		expect(mockPaneLayoutStore.setActiveTab).toHaveBeenCalledWith("group-1", "t1");
	});

	it("does not touch pane layout when not split", () => {
		mockPaneLayoutStore.isSplit.mockReturnValue(false);

		navigateToTerminal("t1");

		expect(mockPaneLayoutStore.setActiveGroup).not.toHaveBeenCalled();
		expect(mockPaneLayoutStore.setActiveTab).not.toHaveBeenCalled();
	});

	it("does not touch pane layout when split but the tab has no group", () => {
		mockPaneLayoutStore.isSplit.mockReturnValue(true);
		mockPaneLayoutStore.getGroupForTab.mockReturnValue(null);

		navigateToTerminal("t1");

		expect(mockPaneLayoutStore.setActiveGroup).not.toHaveBeenCalled();
		expect(mockPaneLayoutStore.setActiveTab).not.toHaveBeenCalled();
	});

	it("opens the manual Global Workspace for an unowned session parked there", () => {
		mockGlobalWorkspaceStore.getScopeMembers.mockReturnValue(["t1"]);

		navigateToTerminal("t1");

		expect(mockGlobalWorkspaceStore.setScope).toHaveBeenCalledWith("manual");
		expect(mockGlobalWorkspaceStore.activate).toHaveBeenCalled();
		expect(mockTerminalsStore.setActive).toHaveBeenCalledWith("t1");
	});

	it("does not throw when the terminal has no ref", () => {
		mockTerminalsStore.get.mockReturnValue(undefined);
		expect(() => navigateToTerminal("missing")).not.toThrow();
	});
});
