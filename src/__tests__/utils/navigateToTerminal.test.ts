import { beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

const { mockRepositoriesStore, mockPaneLayoutStore, mockFocus, mockTerminalsStore } = vi.hoisted(() => {
	const mockRepositoriesStore = {
		getRepoPathForTerminal: vi.fn<(id: string) => string | null>(),
		state: {
			activeRepoPath: null as string | null,
			repositories: {} as Record<
				string,
				{ activeBranch: string | null; branches: Record<string, { terminals: string[] }> }
			>,
		},
		setActive: vi.fn(),
		setActiveBranch: vi.fn(),
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

	return { mockRepositoriesStore, mockPaneLayoutStore, mockFocus, mockTerminalsStore };
});

vi.mock("../../stores/repositories", () => ({ repositoriesStore: mockRepositoriesStore }));
vi.mock("../../stores/paneLayout", () => ({ paneLayoutStore: mockPaneLayoutStore }));
vi.mock("../../stores/terminals", () => ({ terminalsStore: mockTerminalsStore }));

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

	it("does nothing to repo/branch state when the terminal has no owning repo", () => {
		mockRepositoriesStore.getRepoPathForTerminal.mockReturnValue(null);
		navigateToTerminal("t1");
		expect(mockRepositoriesStore.setActive).not.toHaveBeenCalled();
		expect(mockRepositoriesStore.setActiveBranch).not.toHaveBeenCalled();
		expect(mockTerminalsStore.setActive).toHaveBeenCalledWith("t1");
	});

	it("switches repo and branch when the terminal lives in a different repo/branch", () => {
		mockRepositoriesStore.getRepoPathForTerminal.mockReturnValue("/repo/a");
		mockRepositoriesStore.state.activeRepoPath = "/repo/other";
		mockRepositoriesStore.state.repositories = {
			"/repo/a": { activeBranch: "old-branch", branches: { main: { terminals: ["t1"] } } },
		};

		navigateToTerminal("t1");

		expect(mockRepositoriesStore.setActive).toHaveBeenCalledWith("/repo/a");
		expect(mockRepositoriesStore.setActiveBranch).toHaveBeenCalledWith("/repo/a", "main");
	});

	it("skips repo/branch writes when already active", () => {
		mockRepositoriesStore.getRepoPathForTerminal.mockReturnValue("/repo/a");
		mockRepositoriesStore.state.activeRepoPath = "/repo/a";
		mockRepositoriesStore.state.repositories = {
			"/repo/a": { activeBranch: "main", branches: { main: { terminals: ["t1"] } } },
		};

		navigateToTerminal("t1");

		expect(mockRepositoriesStore.setActive).not.toHaveBeenCalled();
		expect(mockRepositoriesStore.setActiveBranch).not.toHaveBeenCalled();
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

	it("does not throw when the terminal has no ref", () => {
		mockTerminalsStore.get.mockReturnValue(undefined);
		expect(() => navigateToTerminal("missing")).not.toThrow();
	});
});
