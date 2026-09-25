import { beforeEach, describe, expect, it, vi } from "vitest";

const {
	mockAgentConfigs,
	mockRemoteAgentConfigs,
	mockContextActions,
	mockPaneLayout,
	mockTerminals,
	mockWriteClipboard,
	mockRpc,
} = vi.hoisted(() => ({
	mockAgentConfigs: { getRunConfigs: vi.fn() },
	mockRemoteAgentConfigs: { getRunConfigs: vi.fn() },
	mockContextActions: { getActions: vi.fn(), getContextActions: vi.fn() },
	mockPaneLayout: { state: { activeGroupId: null as string | null }, isSplit: vi.fn(), canSplit: vi.fn() },
	mockTerminals: {
		state: { activeId: null as string | null },
		getActive: vi.fn(),
		get: vi.fn(),
		update: vi.fn(),
	},
	mockWriteClipboard: vi.fn(),
	mockRpc: vi.fn(),
}));

vi.mock("../../invoke", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../platform", () => ({ getModifierSymbol: () => "⌘" }));
// Two machines: `REMOTE_REPO` is held by another box and answers with its own
// run configs. Which connection a path maps to is resolved by the registry and
// asserted in remoteRepoRouting.test.ts against the URL that leaves the process;
// what matters here is that the menu asks per repository at all.
vi.mock("../../stores/agentConfigs", () => {
	const REMOTE = "/srv/work/api";
	const configsFor = (repoPath?: string | null) => (repoPath === REMOTE ? mockRemoteAgentConfigs : mockAgentConfigs);
	return {
		agentConfigsStore: mockAgentConfigs,
		agentConfigsForRepo: (repoPath?: string | null) => configsFor(repoPath),
		ensureAgentConfigsForRepo: (repoPath?: string | null) => Promise.resolve(configsFor(repoPath)),
	};
});
const REMOTE_REPO = "/srv/work/api";
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
vi.mock("../../transport", () => ({ rpc: mockRpc }));

import { useTerminalContextMenus } from "../../hooks/useTerminalContextMenus";
import { sendCommand } from "../../utils/sendCommand";

function createOptions(available: Array<{ type: string }> = []) {
	return {
		agentDetection: { getAvailable: vi.fn(() => available) },
		gitOps: { handleAddTerminalToWorkspace: vi.fn().mockResolvedValue("new-term") },
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
		mockRemoteAgentConfigs.getRunConfigs.mockReset().mockReturnValue([]);
		mockContextActions.getActions.mockReset().mockReturnValue([]);
		mockContextActions.getContextActions.mockReset().mockReturnValue([]);
		mockPaneLayout.isSplit.mockReset().mockReturnValue(false);
		mockPaneLayout.canSplit.mockReset().mockReturnValue(true);
		mockWriteClipboard.mockClear();
		mockRpc.mockReset().mockImplementation(async (_command: string, args: { args?: string[] }) => args?.args ?? []);
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

	it("creates and configures a branch terminal from the sidebar agent action", async () => {
		mockTerminals.get.mockReturnValue({ tuicSession: "tuic-1" });
		const options = createOptions([{ type: "claude" }]);
		const menus = useTerminalContextMenus(options as never);

		const item = menus.buildSidebarAgentMenuItems("/repo", "feature")[0];
		await item.action();

		expect(options.gitOps.handleAddTerminalToWorkspace).toHaveBeenCalledWith("/repo", "feature");
		expect(mockTerminals.update).toHaveBeenCalledWith(
			"new-term",
			expect.objectContaining({ name: "Claude Code", agentType: "claude", agentLaunchCommand: "claude" }),
		);
	});

	it("runs the selected agent in the active shell and records its resume command", async () => {
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({ agentType: null, commandBlocks: [] });
		mockTerminals.getActive.mockReturnValue({
			id: "term-1",
			ref: {},
			sessionId: "session-1",
			tuicSession: "tuic-1",
			cwd: "/repo",
		});
		const menus = useTerminalContextMenus(createOptions([{ type: "claude" }]) as never);
		const agentMenu = menus.getContextMenuItems().find((item) => item.label === "Agents");
		await agentMenu?.children?.[0]?.action();
		expect(sendCommand).toHaveBeenCalledWith(expect.any(Function), "claude", null, "posix");
		expect(mockTerminals.update).toHaveBeenCalledWith(
			"term-1",
			expect.objectContaining({
				agentLaunchCommand: "claude",
				name: "Claude Code",
			}),
		);
	});

	it.each(["codex", "grok"])("passes native scrollback to %s from the active agent menu", async (agentType) => {
		mockRpc.mockResolvedValueOnce(["--no-alt-screen"]);
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({ agentType: null, commandBlocks: [] });
		mockTerminals.getActive.mockReturnValue({
			id: "term-1",
			ref: {},
			sessionId: "session-1",
			tuicSession: "tuic-1",
			cwd: "/repo",
		});
		const menus = useTerminalContextMenus(createOptions([{ type: agentType }]) as never);
		const agentMenu = menus.getContextMenuItems().find((item) => item.label === "Agents");
		await agentMenu?.children?.[0]?.action();
		expect(sendCommand).toHaveBeenCalledWith(expect.any(Function), `${agentType} --no-alt-screen`, null, "posix");
	});

	/**
	 * The tab runs on the machine that holds the repo, so the command typed into
	 * it has to come from that machine's `agents.json`. The local wrapper would
	 * name a path the remote box does not have.
	 */
	it("launches a remote repo's tab with that machine's run config", async () => {
		mockTerminals.get.mockReturnValue({ tuicSession: "tuic-1" });
		mockAgentConfigs.getRunConfigs.mockReturnValue([
			{ name: "mac", command: "c2", args: ["--model", "opus"], is_default: true },
		]);
		mockRemoteAgentConfigs.getRunConfigs.mockReturnValue([
			{ name: "vps", command: "/opt/claude/bin/claude", args: ["--model", "opus"], is_default: true },
		]);
		const options = createOptions([{ type: "claude" }]);
		const menus = useTerminalContextMenus(options as never);

		await menus.buildSidebarAgentMenuItems(REMOTE_REPO, "feature")[0].action();

		expect(mockTerminals.update).toHaveBeenCalledWith(
			"new-term",
			expect.objectContaining({ agentLaunchCommand: "/opt/claude/bin/claude --model opus" }),
		);
	});

	it("keeps a local repo's tab on the local run config", async () => {
		mockTerminals.get.mockReturnValue({ tuicSession: "tuic-1" });
		mockAgentConfigs.getRunConfigs.mockReturnValue([{ name: "mac", command: "c2", args: [], is_default: true }]);
		mockRemoteAgentConfigs.getRunConfigs.mockReturnValue([
			{ name: "vps", command: "/opt/claude/bin/claude", args: [], is_default: true },
		]);
		const options = createOptions([{ type: "claude" }]);
		const menus = useTerminalContextMenus(options as never);

		await menus.buildSidebarAgentMenuItems("/repo", "feature")[0].action();

		expect(mockTerminals.update).toHaveBeenCalledWith(
			"new-term",
			expect.objectContaining({ agentLaunchCommand: "c2" }),
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
});
