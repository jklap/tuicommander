import { beforeEach, describe, expect, it, vi } from "vitest";

const { mockAgentConfigs, mockContextActions, mockPaneLayout, mockTerminals, mockWriteClipboard } = vi.hoisted(() => ({
	mockAgentConfigs: { getRunConfigs: vi.fn() },
	mockContextActions: { getActions: vi.fn(), getContextActions: vi.fn() },
	mockPaneLayout: { state: { activeGroupId: null as string | null }, isSplit: vi.fn(), canSplit: vi.fn() },
	mockTerminals: {
		state: { activeId: null as string | null },
		getActive: vi.fn(),
		get: vi.fn(),
		update: vi.fn(),
	},
	mockWriteClipboard: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../invoke", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../platform", () => ({ getModifierSymbol: () => "⌘" }));
vi.mock("../../stores/agentConfigs", () => ({ agentConfigsStore: mockAgentConfigs }));
vi.mock("../../stores/contextMenuActionsStore", () => ({ contextMenuActionsStore: mockContextActions }));
vi.mock("../../stores/paneLayout", () => ({ paneLayoutStore: mockPaneLayout }));
vi.mock("../../stores/repositories", () => ({ repositoriesStore: { state: { activeRepoPath: "/repo" } } }));
vi.mock("../../stores/settings", () => ({ settingsStore: { isAgentEnabled: vi.fn(() => true) } }));
vi.mock("../../stores/terminals", () => ({ terminalsStore: mockTerminals }));
vi.mock("../../utils/clipboard", () => ({ writeClipboard: mockWriteClipboard }));
vi.mock("../../utils/hotkey", () => ({ keyFor: (action: string) => action }));
vi.mock("../../utils/sendCommand", () => ({
	getShellFamily: vi.fn().mockResolvedValue("posix"),
	sendCommand: vi.fn().mockResolvedValue(undefined),
}));

import { useTerminalContextMenus } from "../../hooks/useTerminalContextMenus";
import { appLogger } from "../../stores/appLogger";

function createOptions(available: Array<{ type: string }> = []) {
	return {
		agentDetection: { getAvailable: vi.fn(() => available) },
		gitOps: { handleAddTerminalToBranch: vi.fn().mockResolvedValue("new-term") },
		splitPanes: { handleSplit: vi.fn() },
		terminalLifecycle: { copyFromTerminal: vi.fn(), pasteToTerminal: vi.fn(), clearTerminal: vi.fn() },
		closeActiveTabOrPane: vi.fn(),
		setTermRenameDefault: vi.fn(),
		setTermRenamePromptVisible: vi.fn(),
	};
}

describe("useTerminalContextMenus", () => {
	beforeEach(() => {
		mockTerminals.state.activeId = null;
		mockTerminals.get.mockReset();
		mockTerminals.getActive.mockReset();
		mockTerminals.update.mockClear();
		mockAgentConfigs.getRunConfigs.mockReset().mockReturnValue([]);
		mockContextActions.getActions.mockReset().mockReturnValue([]);
		mockContextActions.getContextActions.mockReset().mockReturnValue([]);
		mockPaneLayout.isSplit.mockReset().mockReturnValue(false);
		mockPaneLayout.canSplit.mockReset().mockReturnValue(true);
		mockWriteClipboard.mockReset().mockResolvedValue(undefined);
	});

	it("builds core terminal actions and disables splitting without an active terminal", () => {
		const options = createOptions();
		const menus = useTerminalContextMenus(options as never);

		const items = menus.getContextMenuItems();

		expect(items.map((item) => item.label)).toEqual(
			expect.arrayContaining(["Copy", "Paste", "Split Right", "Clear", "Close Terminal"]),
		);
		expect(items.find((item) => item.label === "Split Right")?.disabled).toBe(true);
	});

	it("copies only the last command block output", async () => {
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({
			commandBlocks: [{ executionLine: 10, endLine: 13 }],
			ref: { getBufferLines: vi.fn().mockResolvedValue(["output", ""]) },
		});
		const items = useTerminalContextMenus(createOptions() as never).getContextMenuItems();

		await items.find((item) => item.label === "Copy Block Output")?.action();

		expect(mockWriteClipboard).toHaveBeenCalledWith("output");
	});

	it("logs instead of throwing when Copy Block Output's clipboard write is denied", async () => {
		const err = new DOMException("Write permission denied.", "NotAllowedError");
		mockWriteClipboard.mockRejectedValueOnce(err);
		const warnSpy = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({
			commandBlocks: [{ executionLine: 10, endLine: 13 }],
			ref: { getBufferLines: vi.fn().mockResolvedValue(["output", ""]) },
		});
		const items = useTerminalContextMenus(createOptions() as never).getContextMenuItems();

		await items.find((item) => item.label === "Copy Block Output")?.action();

		await vi.waitFor(() => {
			expect(warnSpy).toHaveBeenCalledWith("terminal", "Copy Block Output failed to write clipboard", err);
		});
	});

	it("creates and configures a branch terminal from the sidebar agent action", async () => {
		mockTerminals.get.mockReturnValue({ tuicSession: "tuic-1" });
		const options = createOptions([{ type: "claude" }]);
		const menus = useTerminalContextMenus(options as never);

		const item = menus.buildSidebarAgentMenuItems("/repo", "feature")[0];
		await item.action();

		expect(options.gitOps.handleAddTerminalToBranch).toHaveBeenCalledWith("/repo", "feature");
		expect(mockTerminals.update).toHaveBeenCalledWith(
			"new-term",
			expect.objectContaining({ name: "Claude Code", agentType: "claude", agentLaunchCommand: "claude" }),
		);
	});

	it("keeps registered actions and smart prompts in separate groups", () => {
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({ sessionId: "session-1", commandBlocks: [] });
		mockContextActions.getActions.mockReturnValue([{ id: "legacy", label: "Legacy", action: vi.fn() }]);
		mockContextActions.getContextActions.mockImplementation((_target: string, filter: { pluginId?: string }) =>
			filter.pluginId ? [{ id: "prompt", label: "Prompt", action: vi.fn() }] : [],
		);

		const items = useTerminalContextMenus(createOptions() as never).getContextMenuItems();

		expect(items.find((item) => item.label === "Actions")?.children?.map((item) => item.label)).toEqual(["Legacy"]);
		expect(items.find((item) => item.label === "Prompts")?.children?.map((item) => item.label)).toEqual(["Prompt"]);
	});

	it("respects each registered action's own disabled() predicate", () => {
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({ sessionId: "session-1", commandBlocks: [] });
		mockContextActions.getActions.mockReturnValue([
			{ id: "legacy", label: "Legacy", action: vi.fn(), disabled: () => true },
		]);
		mockContextActions.getContextActions.mockImplementation((_target: string, filter: { pluginId?: string }) =>
			filter.pluginId ? [] : [{ id: "plugin", label: "Plugin", action: vi.fn(), disabled: () => true }],
		);

		const items = useTerminalContextMenus(createOptions() as never).getContextMenuItems();
		const actionsChildren = items.find((item) => item.label === "Actions")?.children ?? [];

		expect(actionsChildren.find((c) => c.label === "Legacy")?.disabled).toBe(true);
		expect(actionsChildren.find((c) => c.label === "Plugin")?.disabled).toBe(true);
	});

	it("disables the Agents menu while the active terminal already runs an agent", () => {
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({ agentType: "claude", commandBlocks: [] });
		const options = createOptions([{ type: "claude" }]);

		const items = useTerminalContextMenus(options as never).getContextMenuItems();

		expect(items.find((item) => item.label === "Agents")?.disabled).toBe(true);
	});

	it("builds a submenu of run configs, marking the default one", () => {
		mockAgentConfigs.getRunConfigs.mockReturnValue([
			{ name: "Default run", command: "claude", args: [], is_default: true },
			{ name: "Alt run", command: "claude", args: ["--alt"], is_default: false },
		]);
		const options = createOptions([{ type: "claude" }]);

		const items = useTerminalContextMenus(options as never).getContextMenuItems();
		const agentsChildren = items.find((item) => item.label === "Agents")?.children ?? [];
		const claudeEntry = agentsChildren.find((c) => c.label === "Claude Code");

		expect(claudeEntry?.children?.map((c) => c.label)).toEqual(["Default run (Default)", "Alt run"]);
	});

	it("launches the agent in the active terminal and renames it", async () => {
		mockTerminals.getActive.mockReturnValue({ id: "term-1", ref: {}, sessionId: "session-1", tuicSession: null });
		const options = createOptions([{ type: "claude" }]);

		const items = useTerminalContextMenus(options as never).getContextMenuItems();
		const claudeItem = items.find((item) => item.label === "Agents")?.children?.find((c) => c.label === "Claude Code");
		await claudeItem?.action();

		expect(mockTerminals.update).toHaveBeenCalledWith(
			"term-1",
			expect.objectContaining({ name: "Claude Code", nameIsCustom: true, agentLaunchCommand: "claude" }),
		);
	});

	it("does nothing when launching an agent with no active terminal ref/session", async () => {
		mockTerminals.getActive.mockReturnValue(undefined);
		const options = createOptions([{ type: "claude" }]);

		const items = useTerminalContextMenus(options as never).getContextMenuItems();
		const claudeItem = items.find((item) => item.label === "Agents")?.children?.find((c) => c.label === "Claude Code");
		await claudeItem?.action();

		expect(mockTerminals.update).not.toHaveBeenCalled();
	});

	it("disables Copy Block Output when the active terminal has no command blocks", () => {
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({ commandBlocks: [] });

		const items = useTerminalContextMenus(createOptions() as never).getContextMenuItems();

		expect(items.find((item) => item.label === "Copy Block Output")?.disabled).toBe(true);
	});

	it("Reset Terminal writes the RIS escape sequence to the active terminal", () => {
		const write = vi.fn();
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({ ref: { write }, commandBlocks: [] });

		const items = useTerminalContextMenus(createOptions() as never).getContextMenuItems();
		items.find((item) => item.label === "Reset Terminal")?.action();

		expect(write).toHaveBeenCalledWith("\x1bc");
	});

	it("Change Title… seeds the rename prompt with the terminal's current name and opens it", () => {
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({ name: "my-shell", commandBlocks: [] });
		const options = createOptions();

		const items = useTerminalContextMenus(options as never).getContextMenuItems();
		items.find((item) => item.label === "Change Title…")?.action();

		expect(options.setTermRenameDefault).toHaveBeenCalledWith("my-shell");
		expect(options.setTermRenamePromptVisible).toHaveBeenCalledWith(true);
	});

	it("Change Title… is a no-op with no active terminal", () => {
		mockTerminals.state.activeId = null;
		const options = createOptions();

		const items = useTerminalContextMenus(options as never).getContextMenuItems();
		items.find((item) => item.label === "Change Title…")?.action();

		expect(options.setTermRenameDefault).not.toHaveBeenCalled();
	});

	describe("buildSidebarAgentMenuItems", () => {
		it("returns an empty menu when no agents are enabled", () => {
			const options = createOptions([]);
			const menus = useTerminalContextMenus(options as never);
			expect(menus.buildSidebarAgentMenuItems("/repo", "feature")).toEqual([]);
		});

		it("collapses a single agent with a single run config into one flat item", () => {
			mockAgentConfigs.getRunConfigs.mockReturnValue([]);
			const options = createOptions([{ type: "claude" }]);
			const menus = useTerminalContextMenus(options as never);

			const items = menus.buildSidebarAgentMenuItems("/repo", "feature");

			expect(items).toHaveLength(1);
			expect(items[0].label).toBe("Add Claude Code");
			expect(items[0].children).toBeUndefined();
		});

		it("nests multiple run configs under one agent even when it's the only agent", () => {
			mockAgentConfigs.getRunConfigs.mockReturnValue([
				{ name: "A", command: "claude", args: [], is_default: true },
				{ name: "B", command: "claude", args: [], is_default: false },
			]);
			const options = createOptions([{ type: "claude" }]);
			const menus = useTerminalContextMenus(options as never);

			const items = menus.buildSidebarAgentMenuItems("/repo", "feature");

			expect(items).toHaveLength(1);
			expect(items[0].label).toBe("Add Claude Code");
			expect(items[0].children?.map((c) => c.label)).toEqual(["A (Default)", "B"]);
		});

		it("wraps multiple enabled agents under a single Add Agent submenu", () => {
			mockAgentConfigs.getRunConfigs.mockReturnValue([]);
			const options = createOptions([{ type: "claude" }, { type: "codex" }]);
			const menus = useTerminalContextMenus(options as never);

			const items = menus.buildSidebarAgentMenuItems("/repo", "feature");

			expect(items).toHaveLength(1);
			expect(items[0].label).toBe("Add Agent");
			expect(items[0].children?.map((c) => c.label)).toEqual(["Claude Code", "Codex CLI"]);
		});

		it("filters out agents disabled via settingsStore.isAgentEnabled", async () => {
			const { settingsStore } = await import("../../stores/settings");
			(settingsStore.isAgentEnabled as ReturnType<typeof vi.fn>).mockImplementation((t: string) => t !== "codex");
			mockAgentConfigs.getRunConfigs.mockReturnValue([]);
			const options = createOptions([{ type: "claude" }, { type: "codex" }]);
			const menus = useTerminalContextMenus(options as never);

			const items = menus.buildSidebarAgentMenuItems("/repo", "feature");

			expect(items[0].label).toBe("Add Claude Code");
			(settingsStore.isAgentEnabled as ReturnType<typeof vi.fn>).mockImplementation(() => true);
		});

		it("no-ops when handleAddTerminalToBranch yields no terminal id", async () => {
			mockAgentConfigs.getRunConfigs.mockReturnValue([]);
			const options = createOptions([{ type: "claude" }]);
			options.gitOps.handleAddTerminalToBranch.mockResolvedValue(undefined);
			const menus = useTerminalContextMenus(options as never);

			const item = menus.buildSidebarAgentMenuItems("/repo", "feature")[0];
			await item.action();

			expect(mockTerminals.update).not.toHaveBeenCalled();
		});
	});
});
