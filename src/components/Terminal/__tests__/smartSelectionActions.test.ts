import { describe, expect, it, vi } from "vitest";
import { runSmartSelectionAction, type SmartSelectionActionDeps } from "../smartSelectionActions";
import type { SmartSelectionAction } from "../smartSelectionTypes";

function makeDeps(): SmartSelectionActionDeps {
	return {
		copyToClipboard: vi.fn().mockResolvedValue(undefined),
		openUrl: vi.fn(),
		openFile: vi.fn(),
		sendText: vi.fn().mockResolvedValue(undefined),
		runInNewTerminal: vi.fn().mockResolvedValue(undefined),
		askAi: vi.fn(),
		onBlockedUrl: vi.fn(),
	};
}

function action(overrides: Partial<SmartSelectionAction> = {}): SmartSelectionAction {
	return { kind: "copy", title: "Copy", parameter: "\\0", isDefault: false, ...overrides };
}

const ctx = { matchText: "a1b2c3d", groups: [], cwd: "/repo", user: "jason", host: "box" };

describe("runSmartSelectionAction", () => {
	it("copy: substitutes the parameter and calls copyToClipboard", async () => {
		const deps = makeDeps();
		await runSmartSelectionAction(action({ kind: "copy", parameter: "\\0" }), ctx, deps);
		expect(deps.copyToClipboard).toHaveBeenCalledWith("a1b2c3d");
	});

	it("open_url: allowlisted scheme calls openUrl", async () => {
		const deps = makeDeps();
		await runSmartSelectionAction(action({ kind: "open_url", parameter: "https://\\0" }), ctx, deps);
		expect(deps.openUrl).toHaveBeenCalledWith("https://a1b2c3d");
		expect(deps.onBlockedUrl).not.toHaveBeenCalled();
	});

	it("open_url: disallowed scheme calls onBlockedUrl instead of openUrl", async () => {
		const deps = makeDeps();
		await runSmartSelectionAction(action({ kind: "open_url", parameter: "file:///etc/passwd" }), ctx, deps);
		expect(deps.openUrl).not.toHaveBeenCalled();
		expect(deps.onBlockedUrl).toHaveBeenCalledWith("file:///etc/passwd");
	});

	it("open_url: malformed URL calls onBlockedUrl", async () => {
		const deps = makeDeps();
		await runSmartSelectionAction(action({ kind: "open_url", parameter: "not a url" }), ctx, deps);
		expect(deps.onBlockedUrl).toHaveBeenCalledWith("not a url");
	});

	it("open_file: substitutes the parameter and calls openFile", async () => {
		const deps = makeDeps();
		await runSmartSelectionAction(action({ kind: "open_file", parameter: "\\0" }), ctx, deps);
		expect(deps.openFile).toHaveBeenCalledWith("a1b2c3d");
	});

	it("send_text: calls sendText with autoSubmitAllowed=false (never auto-submits)", async () => {
		const deps = makeDeps();
		await runSmartSelectionAction(action({ kind: "send_text", parameter: "echo \\0" }), ctx, deps);
		expect(deps.sendText).toHaveBeenCalledWith("echo a1b2c3d", false);
	});

	it("run_command: calls sendText with autoSubmitAllowed=true (subject to the caller's metacharacter check)", async () => {
		const deps = makeDeps();
		await runSmartSelectionAction(action({ kind: "run_command", parameter: "git show \\0" }), ctx, deps);
		expect(deps.sendText).toHaveBeenCalledWith("git show a1b2c3d", true);
	});

	it("run_command_new_terminal: substitutes the parameter and calls runInNewTerminal", async () => {
		const deps = makeDeps();
		await runSmartSelectionAction(action({ kind: "run_command_new_terminal", parameter: "git log \\0" }), ctx, deps);
		expect(deps.runInNewTerminal).toHaveBeenCalledWith("git log a1b2c3d");
	});

	it("ask_ai: substitutes the parameter and calls askAi", async () => {
		const deps = makeDeps();
		await runSmartSelectionAction(action({ kind: "ask_ai", parameter: "Explain \\0" }), ctx, deps);
		expect(deps.askAi).toHaveBeenCalledWith("Explain a1b2c3d");
	});

	it("substitutes \\d/\\u/\\h from the context for any action kind", async () => {
		const deps = makeDeps();
		await runSmartSelectionAction(action({ kind: "run_command", parameter: "cd \\d && whoami # \\u@\\h" }), ctx, deps);
		expect(deps.sendText).toHaveBeenCalledWith("cd /repo && whoami # jason@box", true);
	});
});
