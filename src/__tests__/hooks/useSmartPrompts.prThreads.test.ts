/**
 * "Address Review Comments" sends the open review threads (#1870-2bd6). The
 * list itself is rendered and fenced in Rust and tested there; a case here
 * covers what the prompt asks for and that the answer reaches the agent's PTY.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, sendCommand, getBranchPrData } = vi.hoisted(() => ({
	invoke: vi.fn(),
	getBranchPrData: vi.fn((): unknown => null),
	sendCommand: vi.fn(async (_session: string, _text: string) => {}),
}));

vi.mock("../../invoke", () => ({ invoke }));
vi.mock("../../stores/settings", () => ({ settingsStore: { isAcpConfigured: () => true } }));
vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		getActive: () => ({ id: "t1", sessionId: "s1", agentType: "claude", cwd: "/repo" }),
		isBusy: () => false,
	},
}));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: {
		getActive: () => ({ path: "/repo", activeWorkspaceId: "feat" }),
		get: () => undefined,
		state: { repositories: {} },
	},
}));
vi.mock("../../stores/github", () => ({ githubStore: { getBranchPrData } }));
vi.mock("../../stores/promptLibrary", () => ({
	promptLibraryStore: {
		markAsUsed: vi.fn(),
		processContent: async (prompt: { content: string }, vars: Record<string, string>) =>
			prompt.content.replace(/\{(\w+)\}/g, (_, name: string) => vars[name] ?? `{${name}}`),
	},
}));
vi.mock("../../stores/agentConfigs", () => ({
	agentConfigsStore: { getHeadlessAgent: () => null, getHeadlessTemplate: () => undefined, getRunConfigs: () => [] },
}));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));
vi.mock("../../hooks/usePty", () => ({ usePty: () => ({ sendCommand }) }));

import { SMART_PROMPTS_BUILTIN } from "../../data/smartPromptsBuiltIn";
import { useSmartPrompts } from "../../hooks/useSmartPrompts";

const LIST = "Open review threads…\n```untrusted-review-threads\n- a.rs:1 · codex · high · T · u\n```";

/** The backend's `resolve_prompt_variables`: it lists every `{name}` the content uses. */
function backend(threads: () => Promise<string>) {
	invoke.mockImplementation(async (cmd: string, args: { content?: string }) => {
		if (cmd === "resolve_prompt_variables") {
			const needed = [...new Set([...(args.content ?? "").matchAll(/\{(\w+)\}/g)].map((m) => m[1]))];
			return { vars: {}, needed };
		}
		if (cmd === "get_pr_open_threads") return threads();
		throw new Error(`unexpected ${cmd}`);
	});
}

const addressComments = () => {
	const prompt = SMART_PROMPTS_BUILTIN.find((p) => p.id === "smart-review-comments");
	if (!prompt) throw new Error("builtin missing");
	return { ...prompt, autoExecute: true };
};

beforeEach(() => {
	vi.clearAllMocks();
	getBranchPrData.mockReturnValue(null);
});

describe("Address Review Comments", () => {
	// Catches: the prompt still telling the agent to go and find the comments, so
	// the open findings never reach it, or the list fetched for the wrong PR.
	it("sends the open threads of the popover's PR through sendCommand", async () => {
		backend(async () => LIST);
		const result = await useSmartPrompts().executeSmartPrompt(addressComments(), { pr_number: "7" });
		expect(result).toEqual({ ok: true });
		expect(invoke).toHaveBeenCalledWith("get_pr_open_threads", { repoPath: "/repo", prNumber: 7 });
		expect(sendCommand).toHaveBeenCalledTimes(1);
		expect(sendCommand.mock.calls[0][1]).toContain(LIST);
	});

	// Catches: a failed thread read sending the prompt with a literal
	// `{pr_open_threads}` or an empty list, which reads as "nothing to address".
	it("sends nothing when the threads cannot be read", async () => {
		backend(async () => {
			throw new Error("GitHub token missing");
		});
		const result = await useSmartPrompts().executeSmartPrompt(addressComments(), { pr_number: "7" });
		expect(result).toEqual({ ok: false, reason: "Error: GitHub token missing" });
		expect(sendCommand).not.toHaveBeenCalled();
	});

	// Catches: reading a fork PR's threads from the wrong repo when PR numbers collide.
	it("names the branch PR's base repo when its number matches", async () => {
		getBranchPrData.mockReturnValue({ number: 7, base_repo: "org/hello" });
		backend(async () => LIST);
		await useSmartPrompts().executeSmartPrompt(addressComments(), { pr_number: "7" });
		expect(invoke).toHaveBeenCalledWith("get_pr_open_threads", {
			repoPath: "/repo",
			prNumber: 7,
			baseRepo: "org/hello",
		});
	});

	// Catches: a different PR's base repo leaking onto the number the popover asked for.
	it("sends no baseRepo when the branch PR is a different number", async () => {
		getBranchPrData.mockReturnValue({ number: 8, base_repo: "org/hello" });
		backend(async () => LIST);
		await useSmartPrompts().executeSmartPrompt(addressComments(), { pr_number: "7" });
		const call = invoke.mock.calls.find((c) => c[0] === "get_pr_open_threads");
		expect(call).toBeDefined();
		expect((call?.[1] as { baseRepo?: string } | undefined)?.baseRepo).toBeUndefined();
	});

	it("sends no baseRepo when the branch PR has none", async () => {
		getBranchPrData.mockReturnValue({ number: 7 });
		backend(async () => LIST);
		await useSmartPrompts().executeSmartPrompt(addressComments(), { pr_number: "7" });
		const call = invoke.mock.calls.find((c) => c[0] === "get_pr_open_threads");
		expect(call).toBeDefined();
		expect((call?.[1] as { baseRepo?: string } | undefined)?.baseRepo).toBeUndefined();
	});

	// Catches: upstream PR #5's popover reading the threads of the terminal branch's fork PR #5.
	it("prefers the caller's explicit pr_base_repo over a branch PR with the same number", async () => {
		getBranchPrData.mockReturnValue({ number: 5, base_repo: "fork/hello" });
		backend(async () => LIST);
		await useSmartPrompts().executeSmartPrompt(addressComments(), { pr_number: "5", pr_base_repo: "upstream/hello" });
		expect(invoke).toHaveBeenCalledWith("get_pr_open_threads", {
			repoPath: "/repo",
			prNumber: 5,
			baseRepo: "upstream/hello",
		});
	});

	it("falls back to the branch PR when pr_base_repo is empty", async () => {
		getBranchPrData.mockReturnValue({ number: 5, base_repo: "fork/hello" });
		backend(async () => LIST);
		await useSmartPrompts().executeSmartPrompt(addressComments(), { pr_number: "5", pr_base_repo: "" });
		expect(invoke).toHaveBeenCalledWith("get_pr_open_threads", {
			repoPath: "/repo",
			prNumber: 5,
			baseRepo: "fork/hello",
		});
	});

	// Catches: the reserved routing key becoming a usable prompt variable.
	it("does not substitute {pr_base_repo} into a prompt", async () => {
		backend(async () => LIST);
		const prompt = { ...addressComments(), content: "Repo {pr_base_repo}" };
		const result = await useSmartPrompts().executeSmartPrompt(prompt, {
			pr_number: "5",
			pr_base_repo: "upstream/hello",
		});
		expect(result).toEqual({ ok: false, reason: "unresolved_variables", output: JSON.stringify(["pr_base_repo"]) });
		expect(sendCommand).not.toHaveBeenCalled();
	});
});
