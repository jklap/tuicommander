import { beforeEach, describe, expect, it, vi } from "vitest";
import { AGENTS } from "../../agents";
import { buildAgentLaunchCommand, buildResumeCommand, verifyAndBuildResumeCommand } from "../../utils/agentSession";

const { mockAgentConfigsStore } = vi.hoisted(() => ({
	mockAgentConfigsStore: {
		getDefaultConfig: vi.fn().mockReturnValue(undefined),
		getRunConfigs: vi.fn().mockReturnValue([]),
	},
}));

vi.mock("../../stores/agentConfigs", () => ({
	agentConfigsStore: mockAgentConfigsStore,
}));

// Mock rpc for verifyAndBuildResumeCommand tests
const mockRpc = vi.fn();
vi.mock("../../transport", () => ({
	rpc: (...args: unknown[]) => mockRpc(...args),
}));

describe("buildAgentLaunchCommand", () => {
	it("injects --session-id for claude when UUID provided", () => {
		expect(buildAgentLaunchCommand("claude", "abc-123")).toBe("claude --session-id abc-123");
	});

	it("returns bare binary for claude without UUID", () => {
		expect(buildAgentLaunchCommand("claude")).toBe("claude");
	});

	it("returns bare binary for claude with null UUID", () => {
		expect(buildAgentLaunchCommand("claude", null)).toBe("claude");
	});

	it("returns bare binary for non-claude agents even with UUID", () => {
		expect(buildAgentLaunchCommand("gemini", "abc-123")).toBe("gemini");
	});

	it("injects --session-id into full command with args", () => {
		expect(buildAgentLaunchCommand("claude --model opus", "abc-123")).toBe("claude --session-id abc-123 --model opus");
	});

	it("handles command with path prefix", () => {
		expect(buildAgentLaunchCommand("/usr/local/bin/claude", "abc-123")).toBe(
			"/usr/local/bin/claude --session-id abc-123",
		);
	});

	it("handles command with path and args", () => {
		expect(buildAgentLaunchCommand("/usr/local/bin/claude --model sonnet", "uuid-1")).toBe(
			"/usr/local/bin/claude --session-id uuid-1 --model sonnet",
		);
	});
});

describe("buildResumeCommand", () => {
	it("returns --resume <uuid> for claude with UUID", () => {
		expect(buildResumeCommand("claude", "abc-123")).toBe("claude --resume abc-123");
	});

	it("falls back to --continue for claude without UUID", () => {
		expect(buildResumeCommand("claude", null)).toBe("claude --continue");
	});

	it("falls back to --continue for claude with undefined UUID", () => {
		expect(buildResumeCommand("claude")).toBe("claude --continue");
	});

	it("returns id-based resume for gemini with UUID", () => {
		expect(buildResumeCommand("gemini", "abc-123")).toBe("gemini --resume abc-123");
	});

	it("falls back to static resume for gemini without UUID", () => {
		expect(buildResumeCommand("gemini", null)).toBe("gemini --resume");
	});

	it("returns id-based resume for codex with UUID", () => {
		expect(buildResumeCommand("codex", "abc-123")).toBe("codex resume abc-123");
	});

	it("falls back to static resume for codex without UUID", () => {
		expect(buildResumeCommand("codex", null)).toBe("codex resume --last");
	});

	it("returns static resume for aider (no session discovery)", () => {
		expect(buildResumeCommand("aider", null)).toBe("aider --restore-chat-history");
	});

	it("returns static resume for amp", () => {
		expect(buildResumeCommand("amp", null)).toBe("amp threads continue");
	});

	it("returns null for agents without resume support", () => {
		expect(buildResumeCommand("droid", null)).toBeNull();
		expect(buildResumeCommand("git", null)).toBeNull();
	});

	it("uses launchCommand binary instead of default when provided", () => {
		// c is an alias for claude with custom flags; resume must use c, not claude
		expect(buildResumeCommand("claude", "abc-123", "c --dangerously-skip-permissions")).toBe(
			"c --resume abc-123 --dangerously-skip-permissions",
		);
	});

	it("uses plain launchCommand binary when no extra args", () => {
		expect(buildResumeCommand("claude", "abc-123", "c")).toBe("c --resume abc-123");
	});

	it("falls back to default when launchCommand is null", () => {
		expect(buildResumeCommand("claude", "abc-123", null)).toBe("claude --resume abc-123");
	});

	it("keeps the env prefix in front of the binary, not in the resume flags", () => {
		// Discovery rebuilds this from the live process: the alias c2 is gone, and the
		// config dir it hid is stated outright. Resuming without it reads ~/.claude.
		expect(
			buildResumeCommand(
				"claude",
				"abc-123",
				"CLAUDE_CONFIG_DIR=/Users/me/.claude-private claude --dangerously-skip-permissions",
			),
		).toBe("CLAUDE_CONFIG_DIR=/Users/me/.claude-private claude --resume abc-123 --dangerously-skip-permissions");
	});

	it("keeps a quoted env value whole when the path has spaces", () => {
		expect(buildResumeCommand("claude", "abc-123", "CLAUDE_CONFIG_DIR='/Users/me/My Cfg/.claude' claude")).toBe(
			"CLAUDE_CONFIG_DIR='/Users/me/My Cfg/.claude' claude --resume abc-123",
		);
	});
});

describe("sessionDiscovery in AgentConfig", () => {
	it("claude has sessionDiscovery with resumeWithId", () => {
		const disc = AGENTS.claude.sessionDiscovery;
		expect(disc).not.toBeNull();
		expect(disc?.resumeWithId("test-uuid")).toBe("claude --resume test-uuid");
	});

	it("gemini has sessionDiscovery with resumeWithId", () => {
		const disc = AGENTS.gemini.sessionDiscovery;
		expect(disc).not.toBeNull();
		expect(disc?.resumeWithId("test-uuid")).toBe("gemini --resume test-uuid");
	});

	it("codex has sessionDiscovery with resumeWithId", () => {
		const disc = AGENTS.codex.sessionDiscovery;
		expect(disc).not.toBeNull();
		expect(disc?.resumeWithId("test-uuid")).toBe("codex resume test-uuid");
	});

	it("aider has null sessionDiscovery (no session IDs)", () => {
		expect(AGENTS.aider.sessionDiscovery).toBeNull();
	});

	it("amp has null sessionDiscovery (cloud-only)", () => {
		expect(AGENTS.amp.sessionDiscovery).toBeNull();
	});

	it("opencode has null sessionDiscovery (SQLite, not implemented)", () => {
		expect(AGENTS.opencode.sessionDiscovery).toBeNull();
	});
});

describe("verifyAndBuildResumeCommand", () => {
	beforeEach(() => {
		mockRpc.mockReset();
		mockAgentConfigsStore.getDefaultConfig.mockReset().mockReturnValue(undefined);
		mockAgentConfigsStore.getRunConfigs.mockReset().mockReturnValue([]);
	});

	it("uses agentSessionId (not tuicSession) for claude verification", async () => {
		mockRpc.mockResolvedValueOnce(true);
		const result = await verifyAndBuildResumeCommand("claude", "/tmp/repo", "tuic-uuid-1", "discovered-session-id");
		expect(mockRpc).toHaveBeenCalledWith("verify_agent_session", {
			agentType: "claude",
			sessionId: "discovered-session-id",
			cwd: "/tmp/repo",
			agentPid: null,
			envOverrides: {},
		});
		expect(result).toBe("claude --resume discovered-session-id");
	});

	it("returns null when claude agentSessionId not verified (session gone)", async () => {
		mockRpc.mockResolvedValueOnce(false);
		const result = await verifyAndBuildResumeCommand("claude", "/tmp/repo", "tuic-uuid-1", "stale-session");
		expect(result).toBeNull();
	});

	it("returns null when claude has no agentSessionId", async () => {
		const result = await verifyAndBuildResumeCommand("claude", "/tmp/repo", "tuic-uuid-1", null);
		expect(mockRpc).not.toHaveBeenCalled();
		expect(result).toBe("claude --continue");
	});

	it("falls back gracefully when rpc throws (browser mode)", async () => {
		mockRpc.mockRejectedValueOnce(new Error("browser unsupported"));
		const result = await verifyAndBuildResumeCommand("claude", "/tmp/repo", "tuic-uuid-1", "old-session-id");
		expect(result).toBe("claude --resume old-session-id");
	});

	it("uses agentSessionId directly when cwd is null (no verification)", async () => {
		const result = await verifyAndBuildResumeCommand("claude", null, "tuic-uuid-1", "old-session-id");
		expect(mockRpc).not.toHaveBeenCalled();
		expect(result).toBe("claude --resume old-session-id");
	});

	it("skips verification for agents without sessionDiscovery", async () => {
		const result = await verifyAndBuildResumeCommand("aider", "/tmp/repo", "tuic-uuid-1", null);
		expect(mockRpc).not.toHaveBeenCalled();
		expect(result).toBe("aider --restore-chat-history");
	});

	it("returns null for agents without resume support", async () => {
		const result = await verifyAndBuildResumeCommand("droid", "/tmp/repo", "tuic-uuid-1", null);
		expect(result).toBeNull();
	});

	it("verifies gemini agentSessionId instead of tuicSession", async () => {
		mockRpc.mockResolvedValueOnce(true);
		const result = await verifyAndBuildResumeCommand("gemini", "/tmp/repo", "tuic-uuid-1", "discovered-gemini-id");
		expect(mockRpc).toHaveBeenCalledWith("verify_agent_session", {
			agentType: "gemini",
			sessionId: "discovered-gemini-id",
			cwd: "/tmp/repo",
			agentPid: null,
			envOverrides: {},
		});
		expect(result).toBe("gemini --resume discovered-gemini-id");
	});

	it("does not verify a stale Gemini tuicSession when discovery has no agentSessionId", async () => {
		const result = await verifyAndBuildResumeCommand("gemini", "/tmp/repo", "stale-tuic-uuid", null);

		expect(mockRpc).not.toHaveBeenCalled();
		expect(result).toBe("gemini --resume");
	});

	it("preserves persisted Gemini launch args while resuming the discovered ID", async () => {
		mockAgentConfigsStore.getDefaultConfig.mockReturnValue({
			name: "Gemini current",
			command: "gemini",
			args: ["--model", "current-model"],
			env: { HOME: "/tmp/gemini-current-home" },
			is_default: true,
		});
		mockRpc.mockResolvedValueOnce(true);

		const result = await verifyAndBuildResumeCommand("gemini", "/tmp/repo", "stale-tuic-uuid", "discovered-gemini-id");

		expect(mockRpc).toHaveBeenCalledWith("verify_agent_session", {
			agentType: "gemini",
			sessionId: "discovered-gemini-id",
			cwd: "/tmp/repo",
			agentPid: null,
			envOverrides: { HOME: "/tmp/gemini-current-home" },
		});
		expect(result).toBe("gemini --resume discovered-gemini-id --model current-model");
	});

	// The failure this path exists for: the session lives in ~/.claude-private, the
	// default run config is the c2 alias, and TUIC can only tell the two apart from
	// the env the rebuilt launch command carries. Verify in the wrong store and the
	// resume either aims at a session that is not there, or confirms one it will not
	// open — Claude answers "No conversation found with session ID".
	it("verifies against the config dir named by the rebuilt launch command", async () => {
		mockAgentConfigsStore.getDefaultConfig.mockReturnValue({
			name: "Claude Max",
			command: "c2",
			args: [],
			env: {},
			is_default: true,
		});
		mockRpc.mockResolvedValueOnce(true);

		const result = await verifyAndBuildResumeCommand(
			"claude",
			"/tmp/repo",
			"tuic-uuid-1",
			"private-session-id",
			"CLAUDE_CONFIG_DIR=/Users/me/.claude-private claude --dangerously-skip-permissions",
		);

		expect(mockRpc).toHaveBeenCalledWith("verify_agent_session", {
			agentType: "claude",
			sessionId: "private-session-id",
			cwd: "/tmp/repo",
			agentPid: null,
			envOverrides: { CLAUDE_CONFIG_DIR: "/Users/me/.claude-private" },
		});
		expect(result).toBe(
			"CLAUDE_CONFIG_DIR=/Users/me/.claude-private claude --resume private-session-id --dangerously-skip-permissions",
		);
	});

	it("unquotes an env value before handing it to verification", async () => {
		mockRpc.mockResolvedValueOnce(true);

		await verifyAndBuildResumeCommand(
			"claude",
			"/tmp/repo",
			"tuic-uuid-1",
			"private-session-id",
			"CLAUDE_CONFIG_DIR='/Users/me/My Cfg/.claude' claude",
		);

		expect(mockRpc).toHaveBeenCalledWith(
			"verify_agent_session",
			expect.objectContaining({ envOverrides: { CLAUDE_CONFIG_DIR: "/Users/me/My Cfg/.claude" } }),
		);
	});
});
