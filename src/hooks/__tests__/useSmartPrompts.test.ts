/**
 * Tests for resolveHeadlessAgent logic inside useSmartPrompts.
 *
 * resolveHeadlessAgent is a private function — we exercise it through
 * canExecute() with executionMode="headless", which is the only call site.
 * We mock agentConfigsStore, providerRegistryStore, appLogger, and usePty
 * to keep tests focused on the resolution logic.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Must be declared BEFORE importing the module under test.
vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../stores/agentConfigs", () => ({
	agentConfigsStore: {
		getHeadlessAgent: vi.fn(),
		getHeadlessTemplate: vi.fn(),
		getRunConfigs: vi.fn(),
	},
}));

vi.mock("../../stores/providerRegistry", () => ({
	providerRegistryStore: { resolveSlot: vi.fn() },
}));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: { getActive: vi.fn(), isBusy: vi.fn() },
}));

vi.mock("../../stores/github", () => ({
	githubStore: { getBranchPrData: vi.fn() },
}));

vi.mock("../../stores/repositories", () => ({
	// state.repositories defaults empty so resolvePromptTreeIn(cwd, {}) always
	// returns null (no registered repo owns any cwd) — every existing test
	// falls through to the activeRepo-based behavior it already asserted on.
	// Tests exercising the cwd-vs-active-repo fix itself override this.
	repositoriesStore: {
		getActive: vi.fn(),
		getRevision: vi.fn(),
		get: vi.fn(),
		state: { repositories: {} },
	},
}));

vi.mock("../../stores/promptLibrary", () => ({
	promptLibraryStore: {
		processContent: vi.fn(),
		markAsUsed: vi.fn(),
	},
}));

vi.mock("../../utils/promptContext", () => ({
	// prContextVariables is synchronous (plain object -> plain object); the
	// original .mockResolvedValue({}) here made this a Promise-returning stub
	// for a function useSmartPrompts.ts never awaits, silently discarding
	// every prContextVariables(pr) call's actual return value.
	prContextVariables: vi.fn().mockReturnValue({}),
}));

vi.mock("../../transport", () => ({
	isTauri: false,
	rpc: {},
}));

vi.mock("../../platform", () => ({
	isWindows: () => false,
}));

const ptyMocks = vi.hoisted(() => ({
	sendCommand: vi.fn(),
	write: vi.fn(),
}));

vi.mock("../usePty", () => ({
	usePty: vi.fn(() => ({
		createSession: vi.fn(),
		closeSession: vi.fn(),
		sendInput: vi.fn(),
		sendCommand: ptyMocks.sendCommand,
		write: ptyMocks.write,
	})),
}));

import { invoke } from "../../invoke";
import { agentConfigsStore } from "../../stores/agentConfigs";
import { appLogger } from "../../stores/appLogger";
import { githubStore } from "../../stores/github";
import { promptLibraryStore, type SavedPrompt } from "../../stores/promptLibrary";
import { providerRegistryStore } from "../../stores/providerRegistry";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { prContextVariables } from "../../utils/promptContext";
import { resolveInjectTarget, shouldSubmitInjectPrompt, useSmartPrompts } from "../useSmartPrompts";

const mockedGetHeadlessAgent = vi.mocked(agentConfigsStore.getHeadlessAgent);
const mockedGetHeadlessTemplate = vi.mocked(agentConfigsStore.getHeadlessTemplate);
const mockedResolveSlot = vi.mocked(providerRegistryStore.resolveSlot);
const mockedWarn = vi.mocked(appLogger.warn);
const mockedGetActive = vi.mocked(terminalsStore.getActive);
const mockedIsBusy = vi.mocked(terminalsStore.isBusy);
const mockedGetRunConfigs = vi.mocked(agentConfigsStore.getRunConfigs);

/** Build a minimal SavedPrompt fixture with headless executionMode */
function makePrompt(overrides: Partial<SavedPrompt> = {}): SavedPrompt {
	return {
		id: "test-prompt",
		name: "Test Prompt",
		content: "Do something",
		category: "custom",
		isFavorite: false,
		createdAt: 1_000_000,
		updatedAt: 1_000_000,
		executionMode: "headless",
		...overrides,
	};
}

/** Minimal valid resolveSlot result — only the shape matters for canExecute checks */
const CONFIGURED_SLOT = {
	provider: { id: "p1", name: "Test", type: "openai" },
	model: { id: "m1", name: "gpt-4o" },
} as unknown as ReturnType<typeof providerRegistryStore.resolveSlot>;

beforeEach(() => {
	vi.clearAllMocks();
	// Default: headless provider is configured
	mockedResolveSlot.mockReturnValue(CONFIGURED_SLOT);
});

afterEach(() => {
	vi.clearAllMocks();
});

describe("resolveInjectTarget", () => {
	it("resolves an unset target to compose when Compose is open", () => {
		expect(resolveInjectTarget(makePrompt({ injectTarget: undefined }), true)).toBe("compose");
	});

	it("resolves an unset target to terminal when Compose is closed", () => {
		expect(resolveInjectTarget(makePrompt({ injectTarget: undefined }), false)).toBe("terminal");
	});

	it('resolves "auto" the same as unset', () => {
		expect(resolveInjectTarget(makePrompt({ injectTarget: "auto" }), true)).toBe("compose");
		expect(resolveInjectTarget(makePrompt({ injectTarget: "auto" }), false)).toBe("terminal");
	});

	it("an explicit compose target always wins, regardless of whether Compose is open", () => {
		expect(resolveInjectTarget(makePrompt({ injectTarget: "compose" }), false)).toBe("compose");
		expect(resolveInjectTarget(makePrompt({ injectTarget: "compose" }), true)).toBe("compose");
	});

	it("an explicit terminal target always wins, regardless of whether Compose is open", () => {
		expect(resolveInjectTarget(makePrompt({ injectTarget: "terminal" }), false)).toBe("terminal");
		expect(resolveInjectTarget(makePrompt({ injectTarget: "terminal" }), true)).toBe("terminal");
	});
});

describe("shouldSubmitInjectPrompt", () => {
	it("an explicit submitOverride wins over everything else", () => {
		expect(shouldSubmitInjectPrompt(makePrompt({ autoExecute: false, injectTarget: "compose" }), true, true)).toBe(
			true,
		);
		expect(shouldSubmitInjectPrompt(makePrompt({ autoExecute: true, injectTarget: "terminal" }), false, false)).toBe(
			false,
		);
	});

	it("autoExecute wins over the resolved target when no override is given", () => {
		expect(shouldSubmitInjectPrompt(makePrompt({ autoExecute: true, injectTarget: "compose" }), false)).toBe(true);
		expect(shouldSubmitInjectPrompt(makePrompt({ autoExecute: false, injectTarget: "terminal" }), false)).toBe(false);
	});

	it("falls back to the resolved target when autoExecute is unset", () => {
		expect(shouldSubmitInjectPrompt(makePrompt({ autoExecute: undefined, injectTarget: "terminal" }), false)).toBe(
			true,
		);
		expect(shouldSubmitInjectPrompt(makePrompt({ autoExecute: undefined, injectTarget: "compose" }), false)).toBe(
			false,
		);
	});
});

describe("resolveHeadlessAgent — preferred='api'", () => {
	it("returns isApi=true when preferred is 'api'", () => {
		mockedGetHeadlessAgent.mockReturnValue(null);
		const { canExecute } = useSmartPrompts();
		// With isApi=true and a configured headless provider, canExecute returns ok
		const result = canExecute(makePrompt({ preferredAgent: "api" }));
		expect(result.ok).toBe(true);
	});

	it("returns ok=false when preferred='api' but no headless provider configured", () => {
		mockedGetHeadlessAgent.mockReturnValue(null);
		mockedResolveSlot.mockReturnValue(null);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: "api" }));
		expect(result.ok).toBe(false);
		expect(result.reason).toMatch(/Headless provider not configured/);
	});
});

describe("resolveHeadlessAgent — preferred agent with template", () => {
	it("returns the preferred agent when it has a template", () => {
		mockedGetHeadlessTemplate.mockReturnValue("claude --headless {prompt}");
		mockedGetHeadlessAgent.mockReturnValue(null);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: "claude" }));
		expect(result.ok).toBe(true);
		// getHeadlessTemplate must have been called with the preferred agent
		expect(mockedGetHeadlessTemplate).toHaveBeenCalledWith("claude");
	});
});

describe("resolveHeadlessAgent — preferred agent with no template", () => {
	it("falls back to global when preferred has no template", () => {
		mockedGetHeadlessTemplate.mockReturnValue(undefined);
		mockedGetHeadlessAgent.mockReturnValue("gemini");
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: "claude" }));
		// Falls back to global (gemini) which is non-null → ok=true
		expect(result.ok).toBe(true);
	});

	it("logs a warning when falling back to global", () => {
		mockedGetHeadlessTemplate.mockReturnValue(undefined);
		mockedGetHeadlessAgent.mockReturnValue("gemini");
		const { canExecute } = useSmartPrompts();
		canExecute(makePrompt({ preferredAgent: "claude" }));
		expect(mockedWarn).toHaveBeenCalledWith("prompts", expect.stringContaining("claude"));
	});

	it("returns ok=false when preferred has no template and global is null", () => {
		mockedGetHeadlessTemplate.mockReturnValue(undefined);
		mockedGetHeadlessAgent.mockReturnValue(null);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: "claude" }));
		expect(result.ok).toBe(false);
		expect(result.reason).toMatch(/No headless agent configured/);
	});
});

describe("resolveHeadlessAgent — no preferred agent", () => {
	it("returns ok=false when no preferred and global is null", () => {
		mockedGetHeadlessAgent.mockReturnValue(null);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: undefined }));
		expect(result.ok).toBe(false);
		expect(result.reason).toMatch(/No headless agent configured/);
	});

	it("returns ok=true when no preferred and global is a valid agent", () => {
		mockedGetHeadlessAgent.mockReturnValue("claude");
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: undefined }));
		expect(result.ok).toBe(true);
	});

	it("returns isApi=true when no preferred and global='api'", () => {
		mockedGetHeadlessAgent.mockReturnValue("api");
		const { canExecute } = useSmartPrompts();
		// isApi=true triggers the provider check
		const result = canExecute(makePrompt({ preferredAgent: undefined }));
		expect(result.ok).toBe(true);
		expect(mockedResolveSlot).toHaveBeenCalledWith("headless");
	});

	it("returns ok=false when no preferred, global='api', but no headless provider", () => {
		mockedGetHeadlessAgent.mockReturnValue("api");
		mockedResolveSlot.mockReturnValue(null);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: undefined }));
		expect(result.ok).toBe(false);
		expect(result.reason).toMatch(/Headless provider not configured/);
	});
});

describe("canExecuteInject — idle gate by inject target", () => {
	const ACTIVE = { id: "t1", sessionId: "s1", agentType: "claude" } as unknown as ReturnType<
		typeof terminalsStore.getActive
	>;

	beforeEach(() => {
		mockedGetActive.mockReturnValue(ACTIVE);
	});

	it("unset target (auto) with Compose closed resolves to terminal and is gated by a busy agent", () => {
		mockedIsBusy.mockReturnValue(true);
		// ACTIVE has no ref, so isComposeOpen() is unavailable → composeIsOpen defaults to false.
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ executionMode: "inject", injectTarget: undefined }));
		expect(result.ok).toBe(false);
		expect(result.reason).toBe("Agent is busy");
	});

	it("unset target (auto) with Compose already open resolves to compose and is not gated", () => {
		mockedIsBusy.mockReturnValue(true);
		mockedGetActive.mockReturnValue({
			id: "t1",
			sessionId: "s1",
			agentType: "claude",
			ref: { isComposeOpen: () => true },
		} as unknown as ReturnType<typeof terminalsStore.getActive>);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ executionMode: "inject", injectTarget: undefined }));
		expect(result.ok).toBe(true);
		expect(mockedIsBusy).not.toHaveBeenCalled();
	});

	it("compose target with autoExecute=true is gated because it will submit", () => {
		mockedIsBusy.mockReturnValue(true);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ executionMode: "inject", injectTarget: "compose", autoExecute: true }));
		expect(result.ok).toBe(false);
		expect(result.reason).toBe("Agent is busy");
	});

	it("terminal target is blocked while the agent is busy", () => {
		mockedIsBusy.mockReturnValue(true);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ executionMode: "inject", injectTarget: "terminal" }));
		expect(result.ok).toBe(false);
		expect(result.reason).toBe("Agent is busy");
	});

	it("terminal target is allowed when the agent is idle", () => {
		mockedIsBusy.mockReturnValue(false);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ executionMode: "inject", injectTarget: "terminal" }));
		expect(result.ok).toBe(true);
	});

	it("terminal target with requiresIdle=false is allowed even while busy", () => {
		mockedIsBusy.mockReturnValue(true);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ executionMode: "inject", injectTarget: "terminal", requiresIdle: false }));
		expect(result.ok).toBe(true);
	});

	it("terminal target with autoExecute=false is not gated because it only inserts text", () => {
		mockedIsBusy.mockReturnValue(true);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ executionMode: "inject", injectTarget: "terminal", autoExecute: false }));
		expect(result.ok).toBe(true);
		expect(mockedIsBusy).not.toHaveBeenCalled();
	});

	it("requires an active terminal with a detected agent regardless of target", () => {
		mockedGetActive.mockReturnValue(undefined);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ executionMode: "inject", injectTarget: "compose" }));
		expect(result.ok).toBe(false);
		expect(result.reason).toBe("No active terminal");
	});
});

describe("executeInject — routing by inject target", () => {
	const mockedInvoke = vi.mocked(invoke);
	const mockedProcess = vi.mocked(promptLibraryStore.processContent);
	const PROCESSED = "PROCESSED CONTENT";

	/** Active terminal with an optional compose-box ref. */
	const activeWith = (openComposeWithText?: (t: string) => void, isComposeOpen = false) =>
		({
			id: "t1",
			sessionId: "s1",
			agentType: "claude",
			ref: openComposeWithText ? { openComposeWithText, isComposeOpen: () => isComposeOpen } : undefined,
		}) as unknown as ReturnType<typeof terminalsStore.getActive>;

	beforeEach(() => {
		mockedIsBusy.mockReturnValue(false);
		// resolve_prompt_variables → no variables needed.
		mockedInvoke.mockResolvedValue({ vars: {}, needed: [] });
		mockedProcess.mockResolvedValue(PROCESSED);
	});

	it("compose target fills the compose box and never touches the PTY", async () => {
		const openCompose = vi.fn();
		mockedGetActive.mockReturnValue(activeWith(openCompose));
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(makePrompt({ executionMode: "inject", injectTarget: "compose" }));

		expect(res.ok).toBe(true);
		expect(openCompose).toHaveBeenCalledWith(PROCESSED);
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();
		expect(ptyMocks.write).not.toHaveBeenCalled();
	});

	it("autoExecute=true submits even when the unset (auto) inject target would otherwise resolve to compose", async () => {
		const openCompose = vi.fn();
		mockedGetActive.mockReturnValue(activeWith(openCompose, true));
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(
			makePrompt({ executionMode: "inject", injectTarget: undefined, autoExecute: true }),
		);

		expect(res.ok).toBe(true);
		expect(openCompose).not.toHaveBeenCalled();
		expect(ptyMocks.sendCommand).toHaveBeenCalledOnce();
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("s1", PROCESSED, "claude", true);
		expect(ptyMocks.write).not.toHaveBeenCalled();
	});

	it("auto target (unset) with Compose closed routes to the terminal", async () => {
		const openCompose = vi.fn();
		mockedGetActive.mockReturnValue(activeWith(openCompose, false));
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(makePrompt({ executionMode: "inject", injectTarget: undefined }));

		expect(res.ok).toBe(true);
		expect(openCompose).not.toHaveBeenCalled();
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("s1", PROCESSED, "claude", true);
	});

	it("auto target (unset) with Compose already open fills the compose box", async () => {
		const openCompose = vi.fn();
		mockedGetActive.mockReturnValue(activeWith(openCompose, true));
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(makePrompt({ executionMode: "inject", injectTarget: undefined }));

		expect(res.ok).toBe(true);
		expect(openCompose).toHaveBeenCalledWith(PROCESSED);
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();
	});

	it("explicit compose target always fills the compose box regardless of isComposeOpen", async () => {
		const openCompose = vi.fn();
		mockedGetActive.mockReturnValue(activeWith(openCompose, false));
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(makePrompt({ executionMode: "inject", injectTarget: "compose" }));

		expect(res.ok).toBe(true);
		expect(openCompose).toHaveBeenCalledWith(PROCESSED);
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();
	});

	it("explicit terminal target always sends to the agent regardless of isComposeOpen", async () => {
		const openCompose = vi.fn();
		mockedGetActive.mockReturnValue(activeWith(openCompose, true));
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(makePrompt({ executionMode: "inject", injectTarget: "terminal" }));

		expect(res.ok).toBe(true);
		expect(openCompose).not.toHaveBeenCalled();
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("s1", PROCESSED, "claude", true);
	});

	it("terminal target sends straight to the agent via sendCommand", async () => {
		mockedGetActive.mockReturnValue(activeWith());
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(makePrompt({ executionMode: "inject", injectTarget: "terminal" }));

		expect(res.ok).toBe(true);
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("s1", PROCESSED, "claude", true);
		expect(ptyMocks.write).not.toHaveBeenCalled();
	});

	it("terminal target with autoExecute=false uses sendCommand without Enter", async () => {
		mockedGetActive.mockReturnValue(activeWith());
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(
			makePrompt({ executionMode: "inject", injectTarget: "terminal", autoExecute: false }),
		);

		expect(res.ok).toBe(true);
		expect(ptyMocks.sendCommand).toHaveBeenCalledOnce();
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("s1", PROCESSED, "claude", false);
		expect(ptyMocks.write).not.toHaveBeenCalled();
	});

	it("compose target with no compose panel (web/PWA) falls back to a reviewable write", async () => {
		mockedGetActive.mockReturnValue(activeWith()); // no openComposeWithText
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(makePrompt({ executionMode: "inject", injectTarget: "compose" }));

		expect(res.ok).toBe(true);
		expect(ptyMocks.sendCommand).toHaveBeenCalledOnce();
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("s1", PROCESSED, "claude", false);
		expect(ptyMocks.write).not.toHaveBeenCalled();
	});
});

describe("executeHeadless — 'agentType:configName' composite value parsing", () => {
	const mockedInvoke = vi.mocked(invoke);
	const mockedProcess = vi.mocked(promptLibraryStore.processContent);
	const PROCESSED = "PROCESSED CONTENT";

	beforeEach(() => {
		mockedIsBusy.mockReturnValue(false);
		mockedProcess.mockResolvedValue(PROCESSED);
		mockedInvoke.mockImplementation(async (cmd: string) => {
			if (cmd === "resolve_prompt_variables") return { vars: {}, needed: [] };
			if (cmd === "execute_headless_prompt") return "headless output";
			return undefined;
		});
	});

	it("resolves a run config whose name itself contains a colon", async () => {
		// Regression: `"agentType:configName".split(":", 2)` is NOT "split into at
		// most 2 parts" in JS — it splits on every colon and truncates the result
		// array afterward, so a config named "My:Config" (nothing prevents a colon
		// in a run config name) used to have its name silently mangled to "My",
		// failing the `configs.find(...)` lookup and falling back to the agent's
		// bare template instead of the selected config's command/args.
		mockedGetHeadlessAgent.mockReturnValue("claude:My:Config");
		mockedGetRunConfigs.mockReturnValue([
			{ name: "My:Config", command: "my-custom-claude", args: ["--flag", "{prompt}"], env: {}, is_default: false },
		]);
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(makePrompt({ preferredAgent: undefined }));

		expect(res.ok).toBe(true);
		expect(mockedInvoke).toHaveBeenCalledWith(
			"execute_headless_prompt",
			expect.objectContaining({ command: "my-custom-claude", args: ["--flag"] }),
		);
	});

	it("falls back to the agent's template when the composite value's config name isn't found", async () => {
		mockedGetHeadlessAgent.mockReturnValue("claude:Nonexistent Config");
		mockedGetRunConfigs.mockReturnValue([]);
		mockedGetHeadlessTemplate.mockReturnValue('claude --print "{prompt}"');
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(makePrompt({ preferredAgent: undefined }));

		expect(res.ok).toBe(true);
		expect(mockedInvoke).toHaveBeenCalledWith(
			"execute_headless_prompt",
			expect.objectContaining({ command: "claude", args: ["--print"] }),
		);
	});
});

describe("executeSmartPrompt — variable resolution", () => {
	const mockedInvoke = vi.mocked(invoke);
	const mockedProcess = vi.mocked(promptLibraryStore.processContent);
	const mockedGetBranchPrData = vi.mocked(githubStore.getBranchPrData);
	const mockedRepoGet = vi.mocked(repositoriesStore.get);
	const mockedRepoGetActive = vi.mocked(repositoriesStore.getActive);

	const activeTerminal = (overrides: Record<string, unknown> = {}) =>
		({
			id: "t1",
			sessionId: "s1",
			agentType: "claude",
			ref: { openComposeWithText: vi.fn(), isComposeOpen: () => false },
			...overrides,
		}) as unknown as ReturnType<typeof terminalsStore.getActive>;

	beforeEach(() => {
		mockedIsBusy.mockReturnValue(false);
		mockedGetActive.mockReturnValue(activeTerminal());
		mockedRepoGetActive.mockReturnValue({ path: "/repo" } as unknown as ReturnType<typeof repositoriesStore.getActive>);
		mockedRepoGet.mockReturnValue(undefined);
		mockedGetBranchPrData.mockReturnValue(null);
		mockedProcess.mockResolvedValue("PROCESSED");
	});

	it("passes the active repo path to resolve_prompt_variables", async () => {
		mockedInvoke.mockResolvedValue({ vars: {}, needed: [] });
		const { executeSmartPrompt } = useSmartPrompts();

		await executeSmartPrompt(makePrompt({ executionMode: "inject" }));

		expect(mockedInvoke).toHaveBeenCalledWith("resolve_prompt_variables", {
			content: "Do something",
			repoPath: "/repo",
		});
	});

	it("manual variables win over frontend vars, which win over git vars", async () => {
		mockedInvoke.mockResolvedValue({
			vars: { branch: "git-value", shared: "from-git" },
			needed: ["branch", "shared", "agent_type"],
		});
		mockedGetActive.mockReturnValue(activeTerminal({ agentType: "shared-from-frontend" }));

		const { executeSmartPrompt } = useSmartPrompts();
		await executeSmartPrompt(makePrompt({ executionMode: "inject", content: "{branch} {shared} {agent_type}" }), {
			shared: "from-manual",
		});

		const passedVars = mockedProcess.mock.calls[0]?.[1] as Record<string, string>;
		expect(passedVars.branch).toBe("git-value");
		expect(passedVars.shared).toBe("from-manual");
		expect(passedVars.agent_type).toBe("shared-from-frontend");
	});

	it("returns unresolved_variables listing the missing names", async () => {
		mockedInvoke.mockResolvedValue({ vars: {}, needed: ["nonexistent_var", "another_missing"] });
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(makePrompt({ executionMode: "inject" }));

		expect(res.ok).toBe(false);
		if (!res.ok) {
			expect(res.reason).toBe("unresolved_variables");
			expect(JSON.parse(res.output ?? "[]")).toEqual(["nonexistent_var", "another_missing"]);
		}
	});

	it("shell mode routes through shellSafe processContent", async () => {
		mockedInvoke.mockResolvedValue({ vars: {}, needed: [] });
		const { executeSmartPrompt } = useSmartPrompts();

		await executeSmartPrompt(makePrompt({ executionMode: "shell" }));

		expect(mockedProcess).toHaveBeenCalledWith(
			expect.anything(),
			expect.anything(),
			expect.objectContaining({ shellSafe: true }),
		);
	});
});

describe("resolveFrontendVars (via executeSmartPrompt)", () => {
	const mockedInvoke = vi.mocked(invoke);
	const mockedProcess = vi.mocked(promptLibraryStore.processContent);
	const mockedGetBranchPrData = vi.mocked(githubStore.getBranchPrData);
	const mockedRepoGet = vi.mocked(repositoriesStore.get);
	const mockedRepoGetActive = vi.mocked(repositoriesStore.getActive);
	const mockedPrContextVariables = vi.mocked(prContextVariables);

	beforeEach(() => {
		mockedIsBusy.mockReturnValue(false);
		mockedRepoGetActive.mockReturnValue({ path: "/repo" } as unknown as ReturnType<typeof repositoriesStore.getActive>);
		mockedProcess.mockResolvedValue("PROCESSED");
		mockedInvoke.mockResolvedValue({ vars: {}, needed: ["agent_type", "cwd", "pr_title"] });
	});

	it("resolves agent_type and cwd from the active terminal", async () => {
		// canExecuteInject requires active.agentType to be set ("No agent
		// detected in terminal" otherwise) — every fixture below needs it.
		mockedRepoGet.mockReturnValue(undefined);
		mockedGetBranchPrData.mockReturnValue(null);
		mockedGetActive.mockReturnValue({
			id: "t1",
			sessionId: "s1",
			agentType: "claude",
			cwd: "/repo/subdir",
			ref: { openComposeWithText: vi.fn(), isComposeOpen: () => false },
		} as unknown as ReturnType<typeof terminalsStore.getActive>);
		// Only agent_type/cwd are referenced by this test's content — pr_title
		// (in the describe-level default) would otherwise trigger
		// unresolved_variables since no PR is configured here.
		mockedInvoke.mockResolvedValue({ vars: {}, needed: ["agent_type", "cwd"] });

		const { executeSmartPrompt } = useSmartPrompts();
		await executeSmartPrompt(makePrompt({ executionMode: "inject", content: "{agent_type} {cwd}" }));

		const passedVars = mockedProcess.mock.calls[0]?.[1] as Record<string, string>;
		expect(passedVars.agent_type).toBe("claude");
		expect(passedVars.cwd).toBe("/repo/subdir");
	});

	it("resolves pr_* variables only when the active branch has a PR", async () => {
		mockedRepoGet.mockReturnValue({ activeBranch: "feature-x" } as unknown as ReturnType<typeof repositoriesStore.get>);
		mockedGetBranchPrData.mockReturnValue({ title: "My PR" } as unknown as ReturnType<
			typeof githubStore.getBranchPrData
		>);
		mockedPrContextVariables.mockReturnValue({ pr_title: "My PR" });
		mockedGetActive.mockReturnValue({
			id: "t1",
			sessionId: "s1",
			agentType: "claude",
			ref: { openComposeWithText: vi.fn(), isComposeOpen: () => false },
		} as unknown as ReturnType<typeof terminalsStore.getActive>);
		mockedInvoke.mockResolvedValue({ vars: {}, needed: ["pr_title"] });

		const { executeSmartPrompt } = useSmartPrompts();
		await executeSmartPrompt(makePrompt({ executionMode: "inject", content: "{pr_title}" }));

		expect(mockedGetBranchPrData).toHaveBeenCalledWith("/repo", "feature-x");
		const passedVars = mockedProcess.mock.calls[0]?.[1] as Record<string, string>;
		expect(passedVars.pr_title).toBe("My PR");
	});

	it("resolves nothing when the repo has no activeBranch", async () => {
		mockedRepoGet.mockReturnValue(undefined);
		mockedGetActive.mockReturnValue({
			id: "t1",
			sessionId: "s1",
			agentType: "claude",
			ref: { openComposeWithText: vi.fn(), isComposeOpen: () => false },
		} as unknown as ReturnType<typeof terminalsStore.getActive>);

		mockedInvoke.mockResolvedValue({ vars: {}, needed: [] });
		const { executeSmartPrompt } = useSmartPrompts();
		const res = await executeSmartPrompt(makePrompt({ executionMode: "inject" }));

		expect(res.ok).toBe(true);
		expect(mockedGetBranchPrData).not.toHaveBeenCalled();
	});
});

describe("executeSmartPrompt — active-repo-vs-worktree-cwd fix", () => {
	const mockedInvoke = vi.mocked(invoke);
	const mockedProcess = vi.mocked(promptLibraryStore.processContent);
	const mockedGetBranchPrData = vi.mocked(githubStore.getBranchPrData);
	const mockedRepoGetActive = vi.mocked(repositoriesStore.getActive);

	const REPO_ROOT = "/repo";
	const WORKTREE_PATH = "/repo__wt/feat-x";

	beforeEach(() => {
		mockedIsBusy.mockReturnValue(false);
		mockedProcess.mockResolvedValue("PROCESSED");
		mockedInvoke.mockResolvedValue({ vars: {}, needed: [] });
		mockedGetBranchPrData.mockReturnValue(null);
		// A registered repo whose branch "feat-x" is checked out in a linked
		// worktree — the shape resolvePromptTreeIn resolves against.
		(repositoriesStore.state.repositories as Record<string, unknown>) = {
			[REPO_ROOT]: {
				path: REPO_ROOT,
				activeBranch: "main",
				branches: {
					"feat-x": { name: "feat-x", worktreePath: WORKTREE_PATH },
				},
			},
		};
	});

	afterEach(() => {
		(repositoriesStore.state.repositories as Record<string, unknown>) = {};
	});

	it("resolves variables against the focused terminal's worktree, not the active repo", async () => {
		// Active repo (last focused) is unrelated to the worktree the terminal
		// is actually sitting in.
		mockedRepoGetActive.mockReturnValue({ path: "/some-other-repo" } as unknown as ReturnType<
			typeof repositoriesStore.getActive
		>);
		mockedGetActive.mockReturnValue({
			id: "t1",
			sessionId: "s1",
			agentType: "claude",
			cwd: `${WORKTREE_PATH}/src`,
			ref: { openComposeWithText: vi.fn(), isComposeOpen: () => false },
		} as unknown as ReturnType<typeof terminalsStore.getActive>);

		const { executeSmartPrompt } = useSmartPrompts();
		await executeSmartPrompt(makePrompt({ executionMode: "inject" }));

		expect(mockedInvoke).toHaveBeenCalledWith("resolve_prompt_variables", {
			content: "Do something",
			repoPath: WORKTREE_PATH,
		});
	});

	it("falls back to the active repo when the cwd belongs to no registered repo", async () => {
		mockedRepoGetActive.mockReturnValue({ path: REPO_ROOT } as unknown as ReturnType<
			typeof repositoriesStore.getActive
		>);
		mockedGetActive.mockReturnValue({
			id: "t1",
			sessionId: "s1",
			agentType: "claude",
			cwd: "/tmp/some/unregistered/dir",
			ref: { openComposeWithText: vi.fn(), isComposeOpen: () => false },
		} as unknown as ReturnType<typeof terminalsStore.getActive>);

		const { executeSmartPrompt } = useSmartPrompts();
		await executeSmartPrompt(makePrompt({ executionMode: "inject" }));

		expect(mockedInvoke).toHaveBeenCalledWith("resolve_prompt_variables", {
			content: "Do something",
			repoPath: REPO_ROOT,
		});
	});

	it("looks up PR variables under the repo root, not the worktree path", async () => {
		mockedRepoGetActive.mockReturnValue({ path: "/some-other-repo" } as unknown as ReturnType<
			typeof repositoriesStore.getActive
		>);
		mockedGetActive.mockReturnValue({
			id: "t1",
			sessionId: "s1",
			agentType: "claude",
			cwd: `${WORKTREE_PATH}/src`,
			ref: { openComposeWithText: vi.fn(), isComposeOpen: () => false },
		} as unknown as ReturnType<typeof terminalsStore.getActive>);

		const { executeSmartPrompt } = useSmartPrompts();
		await executeSmartPrompt(makePrompt({ executionMode: "inject" }));

		// resolveFrontendVars looks the repo up by REPO ROOT (repositoriesStore
		// is keyed by root, not by worktree path) — passing the worktree path
		// as the first arg here would silently miss and drop every pr_* variable.
		expect(mockedGetBranchPrData).toHaveBeenCalledWith(REPO_ROOT, expect.anything());
	});

	it("looks up PR data for the worktree's own branch, not a stale repo.activeBranch", async () => {
		// Found by a code-review verification pass: resolveFrontendVars used to
		// derive its branch solely from repo.activeBranch, a pointer that
		// navigateToTerminal() keeps in sync but several cross-pane focus paths
		// (Alt+Arrow, closing a split pane) bypass entirely — so it can lag
		// behind which terminal is actually focused. Here activeBranch is
		// deliberately left stale at "main" while the focused terminal's cwd is
		// inside the "feat-x" worktree; pr_* lookups must use "feat-x" (from
		// resolvePromptTreeIn's branchName, derived fresh from the cwd), not
		// the stale "main".
		mockedRepoGetActive.mockReturnValue({ path: "/some-other-repo" } as unknown as ReturnType<
			typeof repositoriesStore.getActive
		>);
		mockedGetActive.mockReturnValue({
			id: "t1",
			sessionId: "s1",
			agentType: "claude",
			cwd: `${WORKTREE_PATH}/src`,
			ref: { openComposeWithText: vi.fn(), isComposeOpen: () => false },
		} as unknown as ReturnType<typeof terminalsStore.getActive>);

		const { executeSmartPrompt } = useSmartPrompts();
		await executeSmartPrompt(makePrompt({ executionMode: "inject" }));

		expect(mockedGetBranchPrData).toHaveBeenCalledWith(REPO_ROOT, "feat-x");
		expect(mockedGetBranchPrData).not.toHaveBeenCalledWith(REPO_ROOT, "main");
	});
});
