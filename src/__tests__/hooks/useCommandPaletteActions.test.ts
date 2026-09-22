import { createRoot } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockContextActions, mockGetActionEntries, mockPluginStore, mockPrompt, mockRepositories, mockTerminals } =
	vi.hoisted(() => ({
		mockContextActions: { getActions: vi.fn() },
		mockGetActionEntries: vi.fn(),
		mockPluginStore: {
			state: { plugins: [] as Array<Record<string, unknown>> },
			getPlugin: vi.fn(),
			setEnabled: vi.fn().mockResolvedValue(undefined),
		},
		mockPrompt: { id: "prompt-1", name: "Review", shortcut: "Cmd+R" },
		mockRepositories: {
			state: {
				repositories: {
					"/active": {
						path: "/active",
						displayName: "Active",
						parked: false,
						activeWorkspaceId: "main",
						workspaces: { main: {} },
					},
					"/parked": {
						path: "/parked",
						displayName: "Parked",
						parked: true,
						activeWorkspaceId: "dev",
						workspaces: { dev: {} },
					},
				},
				groups: { group: { id: "group", name: "Team", repoOrder: ["/active"] } },
			},
			setActive: vi.fn(),
			setPark: vi.fn(),
			isGroupFullyParked: vi.fn(() => false),
			setParkGroup: vi.fn(),
		},
		mockTerminals: { state: { activeId: "term-1" as string | null }, get: vi.fn(), findTerminalWithSession: vi.fn() },
	}));

vi.mock("../../actions/actionRegistry", () => ({ getActionEntries: mockGetActionEntries }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { error: vi.fn(), warn: vi.fn() } }));
vi.mock("../../stores/commandPalette", () => ({ commandPaletteStore: { openWithQuery: vi.fn() } }));
vi.mock("../../stores/contextMenuActionsStore", () => ({ contextMenuActionsStore: mockContextActions }));
vi.mock("../../stores/pluginStore", () => ({ pluginStore: mockPluginStore }));
vi.mock("../../stores/promptLibrary", () => ({
	promptLibraryStore: { getSmartByPlacement: vi.fn(() => [mockPrompt]) },
}));
vi.mock("../../stores/repositories", () => ({ repositoriesStore: mockRepositories }));
vi.mock("../../stores/terminals", () => ({ terminalsStore: mockTerminals }));
vi.mock("../../invoke", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn() } }));
vi.mock("../../stores/updater", () => ({ updaterStore: { checkForUpdate: vi.fn().mockResolvedValue(undefined) } }));

import { useCommandPaletteActions } from "../../hooks/useCommandPaletteActions";
import { invoke } from "../../invoke";
import { toastsStore } from "../../stores/toasts";
import * as transport from "../../transport";

describe("useCommandPaletteActions", () => {
	let dispose: (() => void) | undefined;

	beforeEach(() => {
		mockGetActionEntries.mockReturnValue([
			{ id: "static", label: "Static", category: "Test", keybinding: "", execute: vi.fn() },
		]);
		mockPluginStore.state.plugins = [
			{ id: "plugin-1", enabled: true, builtIn: false, manifest: { name: "Plugin One" } },
		];
		mockPluginStore.getPlugin.mockReturnValue({ id: "plugin-1", enabled: true });
		mockContextActions.getActions.mockReturnValue([
			{ id: "inspect", label: "Inspect", action: vi.fn(), disabled: vi.fn() },
		]);
		mockTerminals.get.mockReturnValue({ sessionId: "session-1" });
		mockTerminals.findTerminalWithSession.mockReturnValue({ sessionId: "session-1", agentType: "claude" });
		vi.clearAllMocks();
	});

	it("starts design mode from the session finder, never the active terminal", async () => {
		mockTerminals.get.mockReturnValue({ sessionId: "wrong-session", agentType: "claude" });
		mockTerminals.findTerminalWithSession.mockReturnValue({ sessionId: "chosen-session", agentType: "claude" });
		let actions: ReturnType<typeof useCommandPaletteActions> | undefined;
		createRoot((rootDispose) => {
			dispose = rootDispose;
			actions = useCommandPaletteActions({
				shortcutHandlers: {} as never,
				gitOps: { getWorktreeTargets: vi.fn(() => []) } as never,
				splitPanes: { resetLayout: vi.fn() } as never,
				executeSmartPrompt: vi.fn().mockResolvedValue(undefined),
			});
		});
		const action = actions?.().find((entry) => entry.id === "start-design-mode");
		expect(action).toBeDefined();
		action?.execute();
		await Promise.resolve();
		expect(invoke).toHaveBeenCalledWith("start_design_mode", { sessionId: "chosen-session" });
		expect(mockTerminals.get).not.toHaveBeenCalled();
	});

	it("reports a design mode start failure with the backend message", async () => {
		vi.mocked(invoke).mockRejectedValueOnce(new Error("No dev server"));
		let actions: ReturnType<typeof useCommandPaletteActions> | undefined;
		createRoot((rootDispose) => {
			dispose = rootDispose;
			actions = useCommandPaletteActions({
				shortcutHandlers: {} as never,
				gitOps: { getWorktreeTargets: vi.fn(() => []) } as never,
				splitPanes: { resetLayout: vi.fn() } as never,
				executeSmartPrompt: vi.fn().mockResolvedValue(undefined),
			});
		});
		actions?.()
			.find((entry) => entry.id === "start-design-mode")
			?.execute();
		await Promise.resolve();
		expect(toastsStore.add).toHaveBeenCalledWith(expect.any(String), "No dev server", "error");
	});

	it("explains the browser host after starting design mode from the palette", async () => {
		const tauri = vi.spyOn(transport, "isTauri").mockReturnValue(false);
		let actions: ReturnType<typeof useCommandPaletteActions> | undefined;
		createRoot((rootDispose) => {
			dispose = rootDispose;
			actions = useCommandPaletteActions({
				shortcutHandlers: {} as never,
				gitOps: { getWorktreeTargets: vi.fn(() => []) } as never,
				splitPanes: { resetLayout: vi.fn() } as never,
				executeSmartPrompt: vi.fn().mockResolvedValue(undefined),
			});
		});
		actions?.()
			.find((entry) => entry.id === "start-design-mode")
			?.execute();
		await Promise.resolve();
		expect(toastsStore.add).toHaveBeenCalledWith("Design Mode", "Chrome opened on the host machine.", "info");
		tauri.mockRestore();
	});

	afterEach(() => {
		dispose?.();
		dispose = undefined;
	});

	it("combines static, repository, plugin, worktree, prompt, and registered actions", () => {
		const gitOps = {
			handleBranchSelect: vi.fn(),
			handleAddRepo: vi.fn(),
			getWorktreeTargets: vi.fn(() => [{ path: "/wt", branchName: "feature" }]),
			moveTerminalToWorktree: vi.fn(),
		};
		let actions: ReturnType<typeof useCommandPaletteActions> | undefined;
		createRoot((rootDispose) => {
			dispose = rootDispose;
			actions = useCommandPaletteActions({
				shortcutHandlers: {} as never,
				gitOps: gitOps as never,
				splitPanes: { resetLayout: vi.fn() } as never,
				executeSmartPrompt: vi.fn().mockResolvedValue(undefined),
			});
		});

		const ids = actions?.().map((action) => action.id);
		expect(ids).toEqual(
			expect.arrayContaining([
				"static",
				"switch-repo:/active",
				"unpark-repo:/parked",
				"park-group:group",
				"toggle-plugin:plugin-1",
				"move-to-worktree:/wt",
				"smart:prompt-1",
				"plugin-action:inspect",
			]),
		);
	});

	it("keeps dynamic action closures bound to the represented entity", async () => {
		const pluginAction = vi.fn();
		mockContextActions.getActions.mockReturnValue([{ id: "inspect", label: "Inspect", action: pluginAction }]);
		const executeSmartPrompt = vi.fn().mockResolvedValue(undefined);
		const gitOps = {
			handleBranchSelect: vi.fn(),
			handleAddRepo: vi.fn(),
			getWorktreeTargets: vi.fn(() => [{ path: "/wt", branchName: "feature" }]),
			moveTerminalToWorktree: vi.fn(),
		};
		let actions: ReturnType<typeof useCommandPaletteActions> | undefined;
		createRoot((rootDispose) => {
			dispose = rootDispose;
			actions = useCommandPaletteActions({
				shortcutHandlers: {} as never,
				gitOps: gitOps as never,
				splitPanes: { resetLayout: vi.fn() } as never,
				executeSmartPrompt,
			});
		});

		const byId = (id: string) =>
			actions?.()
				.find((action) => action.id === id)
				?.execute();
		byId("switch-repo:/active");
		byId("unpark-repo:/parked");
		byId("toggle-plugin:plugin-1");
		byId("move-to-worktree:/wt");
		byId("smart:prompt-1");
		byId("plugin-action:inspect");
		await Promise.resolve();

		expect(gitOps.handleBranchSelect).toHaveBeenNthCalledWith(1, "/active", "main");
		expect(gitOps.handleBranchSelect).toHaveBeenNthCalledWith(2, "/parked", "dev");
		expect(mockPluginStore.setEnabled).toHaveBeenCalledWith("plugin-1", false);
		expect(gitOps.moveTerminalToWorktree).toHaveBeenCalledWith("term-1", "/wt");
		expect(executeSmartPrompt).toHaveBeenCalledWith(mockPrompt);
		expect(pluginAction).toHaveBeenCalledWith({ sessionId: "session-1", repoPath: null });
	});
});
