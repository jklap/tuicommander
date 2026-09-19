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
	mockWriteClipboard: vi.fn().mockResolvedValue(undefined),
	mockRpc: vi.fn(),
}));

vi.mock("../../invoke", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../platform", () => ({ getModifierSymbol: () => "⌘", isWindows: vi.fn(() => false) }));
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
vi.mock("../../stores/settings", () => ({
	settingsStore: { state: { shell: null as string | null }, isAgentEnabled: vi.fn(() => true) },
}));
vi.mock("../../stores/terminals", () => ({
	terminalsStore: mockTerminals,
	// The real filter is a pure function with no store dependency — reuse it
	// rather than re-implementing "filter out onAltScreen" a second time here.
	rowAnchoredBlocks: (blocks: Array<{ onAltScreen?: boolean }>) => blocks.filter((b) => !b.onAltScreen),
}));
vi.mock("../../utils/clipboard", () => ({
	writeClipboard: mockWriteClipboard,
	// Mirrors writeClipboardAsync's real Tauri-mode behavior (this suite runs
	// under setup.ts's default __TAURI_INTERNALS__): await the promise, then
	// route through the same mocked writeClipboard these tests already assert on.
	writeClipboardAsync: async (textPromise: Promise<string>) => mockWriteClipboard(await textPromise),
}));
vi.mock("../../utils/hotkey", () => ({ keyFor: (action: string) => action }));
vi.mock("../../utils/sendCommand", () => ({
	getShellFamily: vi.fn().mockResolvedValue("posix"),
	sendCommand: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../transport", () => ({ rpc: mockRpc }));

import { useTerminalContextMenus } from "../../hooks/useTerminalContextMenus";
import { isWindows } from "../../platform";
import { appLogger } from "../../stores/appLogger";
import { settingsStore } from "../../stores/settings";
import { getShellFamily, sendCommand } from "../../utils/sendCommand";

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
		mockWriteClipboard.mockReset().mockResolvedValue(undefined);
		mockRpc.mockReset().mockImplementation(async (_command: string, args: { args?: string[] }) => args?.args ?? []);
		vi.mocked(getShellFamily).mockResolvedValue("posix");
		vi.mocked(isWindows).mockReturnValue(false);
		settingsStore.state.shell = null;
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
			historyBase: 0,
			commandBlocks: [{ executionLine: 10, endLine: 13 }],
			ref: { getBufferLines: vi.fn().mockResolvedValue(["output", ""]) },
		});
		const items = useTerminalContextMenus(createOptions() as never).getContextMenuItems();

		items.find((item) => item.label === "Copy Block Output")?.action();

		await vi.waitFor(() => expect(mockWriteClipboard).toHaveBeenCalledWith("output"));
	});

	// Catches: all-time block coordinates are passed to the retained-grid API after eviction.
	it("copies the retained part of a block and ignores fully evicted output", async () => {
		mockTerminals.state.activeId = "term-1";
		const getBufferLines = vi.fn().mockResolvedValue(["retained"]);
		const term = { historyBase: 12, commandBlocks: [{ executionLine: 10, endLine: 15 }], ref: { getBufferLines } };
		mockTerminals.get.mockReturnValue(term);
		const items = useTerminalContextMenus(createOptions() as never).getContextMenuItems();
		await items.find((item) => item.label === "Copy Block Output")?.action();
		expect(getBufferLines).toHaveBeenCalledWith(0, 3);
		getBufferLines.mockClear();
		term.historyBase = 15;
		await items.find((item) => item.label === "Copy Block Output")?.action();
		expect(getBufferLines).not.toHaveBeenCalled();
	});

	it("skips a trailing alt-screen-tainted block and copies the last real one instead", async () => {
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({
			commandBlocks: [
				{ executionLine: 10, endLine: 13, onAltScreen: false },
				// A Claude Code fullscreen turn that ran after the last real shell
				// block — its executionLine/endLine are not valid buffer rows.
				{ executionLine: 2, endLine: 4, onAltScreen: true },
			],
			historyBase: 0,
			ref: { getBufferLines: vi.fn().mockResolvedValue(["output", ""]) },
		});
		const items = useTerminalContextMenus(createOptions() as never).getContextMenuItems();

		items.find((item) => item.label === "Copy Block Output")?.action();

		await vi.waitFor(() => expect(mockWriteClipboard).toHaveBeenCalledWith("output"));
	});

	it("disables Copy Block Output when every command block is alt-screen-tainted", () => {
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({
			commandBlocks: [{ executionLine: 2, endLine: 4, onAltScreen: true }],
		});

		const items = useTerminalContextMenus(createOptions() as never).getContextMenuItems();

		expect(items.find((item) => item.label === "Copy Block Output")?.disabled).toBe(true);
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

		items.find((item) => item.label === "Copy Block Output")?.action();

		await vi.waitFor(() => {
			expect(warnSpy).toHaveBeenCalledWith("terminal", "Copy Block Output failed", {
				error: expect.stringContaining("NotAllowedError"),
			});
		});
	});

	it("logs instead of throwing when getBufferLines itself throws synchronously", async () => {
		const warnSpy = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({
			commandBlocks: [{ executionLine: 10, endLine: 13 }],
			ref: {
				getBufferLines: vi.fn().mockImplementation(() => {
					throw new Error("buffer gone");
				}),
				getHistoryBase: () => 0,
			},
		});
		const items = useTerminalContextMenus(createOptions() as never).getContextMenuItems();

		expect(() => items.find((item) => item.label === "Copy Block Output")?.action()).not.toThrow();

		await vi.waitFor(() => {
			expect(warnSpy).toHaveBeenCalledWith("terminal", "Copy Block Output failed", {
				error: expect.stringContaining("buffer gone"),
			});
		});
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

	// Catches: the active menu drops env or splits a quoted value into shell tokens.
	it("scopes a quoted run-config value to the active POSIX agent command", async () => {
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({ agentType: null, commandBlocks: [] });
		mockTerminals.getActive.mockReturnValue({ id: "term-1", ref: {}, sessionId: "session-1", cwd: "/repo" });
		mockAgentConfigs.getRunConfigs.mockReturnValue([
			{
				name: "private",
				command: "claude",
				args: [],
				env: { CLAUDE_CONFIG_DIR: "/my 'private' config" },
				is_default: true,
			},
		]);

		const menu = useTerminalContextMenus(createOptions([{ type: "claude" }]) as never)
			.getContextMenuItems()
			.find((item) => item.label === "Agents");
		await menu?.children?.[0]?.action();

		expect(sendCommand).toHaveBeenCalledWith(
			expect.any(Function),
			"env CLAUDE_CONFIG_DIR='/my '\\''private'\\'' config' claude",
			null,
			"posix",
		);
		expect(mockTerminals.update).toHaveBeenCalledWith(
			"term-1",
			expect.objectContaining({ agentLaunchCommand: "claude" }),
		);
	});

	// Catches: sidebar launch reads this desktop's config or exports into the new shell.
	it("uses the selected remote run-config environment only for the sidebar agent", async () => {
		mockTerminals.get.mockReturnValue({ tuicSession: "tuic-1" });
		mockRemoteAgentConfigs.getRunConfigs.mockReturnValue([
			{ name: "remote", command: "claude", args: [], env: { PROFILE: "a b" }, is_default: true },
		]);

		await useTerminalContextMenus(createOptions([{ type: "claude" }]) as never)
			.buildSidebarAgentMenuItems(REMOTE_REPO, "feature")[0]
			.action();

		expect(mockTerminals.update).toHaveBeenCalledWith(
			"new-term",
			expect.objectContaining({ pendingInitCommand: "env PROFILE='a b' claude", agentLaunchCommand: "claude" }),
		);
	});

	// Catches: a menu config spoofs TUIC identity or treats an empty value as absent.
	it.each(["active", "sidebar"])(
		"protects peer identity while retaining an empty value in %s launch overrides",
		async (path) => {
			const config = {
				name: "private",
				command: "claude",
				args: [],
				env: { TUIC_SESSION: "spoof", TUIC_PARENT: "spoof", EMPTY: "" },
				is_default: true,
			};
			mockAgentConfigs.getRunConfigs.mockReturnValue([config]);
			mockTerminals.state.activeId = "term-1";
			mockTerminals.get.mockReturnValue({ tuicSession: "tuic-1", agentType: null, commandBlocks: [] });
			mockTerminals.getActive.mockReturnValue({ id: "term-1", ref: {}, sessionId: "session-1", cwd: "/repo" });
			const menus = useTerminalContextMenus(createOptions([{ type: "claude" }]) as never);
			if (path === "active") {
				await menus
					.getContextMenuItems()
					.find((item) => item.label === "Agents")
					?.children?.[0]?.action();
				expect(sendCommand).toHaveBeenCalledWith(expect.any(Function), "env EMPTY='' claude", null, "posix");
			} else {
				await menus.buildSidebarAgentMenuItems("/repo", "feature")[0].action();
				expect(mockTerminals.update).toHaveBeenCalledWith(
					"new-term",
					expect.objectContaining({ pendingInitCommand: "env EMPTY='' claude" }),
				);
			}
		},
	);

	// Catches: submenu actions reuse the first run config's environment.
	it("uses the selected active submenu's env instead of another run config's env", async () => {
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({ agentType: null, commandBlocks: [] });
		mockTerminals.getActive.mockReturnValue({ id: "term-1", ref: {}, sessionId: "session-1", cwd: "/repo" });
		mockAgentConfigs.getRunConfigs.mockReturnValue([
			{ name: "first", command: "claude", args: [], env: { PROFILE: "first" }, is_default: true },
			{ name: "second", command: "claude", args: [], env: { PROFILE: "second" }, is_default: false },
		]);

		await useTerminalContextMenus(createOptions([{ type: "claude" }]) as never)
			.getContextMenuItems()
			.find((item) => item.label === "Agents")
			?.children?.[0]?.children?.[1]?.action();

		expect(sendCommand).toHaveBeenCalledWith(expect.any(Function), "env PROFILE='second' claude", null, "posix");
	});

	// Catches: cmd or PowerShell parses an unquoted value before the agent starts.
	it("launches the active Windows agent with scoped environment without exposing a quoted value in the shell line", async () => {
		vi.mocked(isWindows).mockReturnValue(true);
		vi.mocked(getShellFamily).mockResolvedValue("windows-native");
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({ agentType: null, commandBlocks: [] });
		mockTerminals.getActive.mockReturnValue({ id: "term-1", ref: {}, sessionId: "session-1", cwd: "/repo" });
		mockAgentConfigs.getRunConfigs.mockReturnValue([
			{
				name: "private",
				command: "claude",
				args: [],
				env: { PROFILE: "a 'quote' & space", TUIC_PARENT: "spoof" },
				is_default: true,
			},
		]);

		await useTerminalContextMenus(createOptions([{ type: "claude" }]) as never)
			.getContextMenuItems()
			.find((item) => item.label === "Agents")
			?.children?.[0]?.action();

		const line = vi.mocked(sendCommand).mock.calls.at(-1)?.[1] ?? "";
		expect(line).toMatch(/^powershell\.exe -NoProfile -EncodedCommand [A-Za-z0-9+/=]+$/);
		expect(line).not.toContain("a 'quote' & space");
		const script = Buffer.from(line.split(" ").at(-1) ?? "", "base64").toString("utf16le");
		expect(script).toContain("PROFILE");
		expect(script).toContain("a ''quote'' & space");
		expect(script).toContain("claude");
		expect(script).not.toContain("TUIC_PARENT");
	});

	// Catches: Windows accepts a differently cased run-config key as a TUIC identity override.
	it.each(["tuic_session", "TuIc_SeSsIoN", "tuic_parent"])(
		"omits protected Windows identity override %s while retaining an ordinary variable",
		async (protectedKey) => {
			vi.mocked(isWindows).mockReturnValue(true);
			vi.mocked(getShellFamily).mockResolvedValue("windows-native");
			mockTerminals.state.activeId = "term-1";
			mockTerminals.get.mockReturnValue({ agentType: null, commandBlocks: [] });
			mockTerminals.getActive.mockReturnValue({ id: "term-1", ref: {}, sessionId: "session-1", cwd: "/repo" });
			mockAgentConfigs.getRunConfigs.mockReturnValue([
				{
					name: "private",
					command: "claude",
					args: [],
					env: { [protectedKey]: "spoof", PROFILE: "safe" },
					is_default: true,
				},
			]);

			await useTerminalContextMenus(createOptions([{ type: "claude" }]) as never)
				.getContextMenuItems()
				.find((item) => item.label === "Agents")
				?.children?.[0]?.action();

			const line = vi.mocked(sendCommand).mock.calls.at(-1)?.[1] ?? "";
			const script = Buffer.from(line.split(" ").at(-1) ?? "", "base64").toString("utf16le");
			expect(script).toBe("$env:PROFILE = 'safe'; Invoke-Expression 'claude'");
		},
	);

	// Catches: code-point iteration drops the low surrogate from non-BMP text.
	it.each([
		{
			value: "prefix😀suffix",
			command: "claude",
			expected: "$env:PROFILE = 'prefix😀suffix'; Invoke-Expression 'claude'",
		},
		{ value: "😀", command: "claude", expected: "$env:PROFILE = '😀'; Invoke-Expression 'claude'" },
		{ value: "plain", command: "claude 😀", expected: "$env:PROFILE = 'plain'; Invoke-Expression 'claude 😀'" },
	])("preserves UTF-16LE astral text in Windows menu launch: $expected", async ({ value, command, expected }) => {
		vi.mocked(isWindows).mockReturnValue(true);
		vi.mocked(getShellFamily).mockResolvedValue("windows-native");
		mockTerminals.state.activeId = "term-1";
		mockTerminals.get.mockReturnValue({ agentType: null, commandBlocks: [] });
		mockTerminals.getActive.mockReturnValue({ id: "term-1", ref: {}, sessionId: "session-1", cwd: "/repo" });
		mockAgentConfigs.getRunConfigs.mockReturnValue([
			{ name: "private", command, args: [], env: { PROFILE: value }, is_default: true },
		]);

		await useTerminalContextMenus(createOptions([{ type: "claude" }]) as never)
			.getContextMenuItems()
			.find((item) => item.label === "Agents")
			?.children?.[0]?.action();

		const line = vi.mocked(sendCommand).mock.calls.at(-1)?.[1] ?? "";
		const script = Buffer.from(line.split(" ").at(-1) ?? "", "base64").toString("utf16le");
		expect(script).toBe(expected);
	});

	// Catches: the sidebar assumes POSIX syntax on native Windows shells.
	it.each(["cmd.exe", "powershell.exe"])("launches a sidebar agent with scoped env in %s", async (shell) => {
		vi.mocked(isWindows).mockReturnValue(true);
		settingsStore.state.shell = shell;
		mockTerminals.get.mockReturnValue({ tuicSession: "tuic-1" });
		mockAgentConfigs.getRunConfigs.mockReturnValue([
			{
				name: "private",
				command: "claude",
				args: ["--model", "opus"],
				env: { PROFILE: "a b", ICON: "😀", tuic_parent: "spoof" },
				is_default: true,
			},
		]);

		await useTerminalContextMenus(createOptions([{ type: "claude" }]) as never)
			.buildSidebarAgentMenuItems("/repo", "feature")[0]
			.action();

		const update = mockTerminals.update.mock.calls.at(-1)?.[1];
		const line = update?.pendingInitCommand ?? "";
		expect(line).toMatch(/^powershell\.exe -NoProfile -EncodedCommand [A-Za-z0-9+/=]+$/);
		expect(line).not.toContain("a b");
		const script = Buffer.from(line.split(" ").at(-1) ?? "", "base64").toString("utf16le");
		expect(script).toBe("$env:PROFILE = 'a b'; $env:ICON = '😀'; Invoke-Expression 'claude --model opus'");
		expect(update?.agentLaunchCommand).toBe("claude --model opus");
	});

	// Catches: host Windows detection selects PowerShell syntax for Git Bash.
	it("uses POSIX quoting for a Windows Git Bash sidebar terminal", async () => {
		vi.mocked(isWindows).mockReturnValue(true);
		settingsStore.state.shell = "C:\\Program Files\\Git\\bin\\bash.exe";
		mockTerminals.get.mockReturnValue({ tuicSession: "tuic-1" });
		mockAgentConfigs.getRunConfigs.mockReturnValue([
			{ name: "bash", command: "claude", args: [], env: { PROFILE: "a b" }, is_default: true },
		]);

		await useTerminalContextMenus(createOptions([{ type: "claude" }]) as never)
			.buildSidebarAgentMenuItems("/repo", "feature")[0]
			.action();

		expect(mockTerminals.update).toHaveBeenCalledWith(
			"new-term",
			expect.objectContaining({ pendingInitCommand: "env PROFILE='a b' claude" }),
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

		it("no-ops when handleAddTerminalToWorkspace yields no terminal id", async () => {
			mockAgentConfigs.getRunConfigs.mockReturnValue([]);
			const options = createOptions([{ type: "claude" }]);
			options.gitOps.handleAddTerminalToWorkspace.mockResolvedValue(undefined);
			const menus = useTerminalContextMenus(options as never);

			const item = menus.buildSidebarAgentMenuItems("/repo", "feature")[0];
			await item.action();

			expect(mockTerminals.update).not.toHaveBeenCalled();
		});
	});
});
