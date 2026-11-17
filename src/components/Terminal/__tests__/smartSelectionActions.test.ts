import { describe, expect, it, vi } from "vitest";
import { shVar } from "../../../__tests__/helpers/shellVar";
import {
	backgroundFailureMessage,
	runSmartSelectionAction,
	type SmartSelectionActionContext,
	type SmartSelectionActionDeps,
} from "../smartSelectionActions";
import type { SmartSelectionAction } from "../smartSelectionTypes";

function makeDeps(): SmartSelectionActionDeps {
	return {
		copyToClipboard: vi.fn().mockResolvedValue(undefined),
		openUrl: vi.fn(),
		openFile: vi.fn(),
		sendText: vi.fn().mockResolvedValue(undefined),
		runInNewTerminal: vi.fn().mockResolvedValue(undefined),
		runBackground: vi.fn().mockResolvedValue(undefined),
		newTerminalShell: vi.fn().mockResolvedValue({ shell: "zsh", supported: true }),
		onUnsupportedNewTerminalShell: vi.fn(),
		onNewTerminalRefused: vi.fn(),
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

	it("copy: propagates a rejected copyToClipboard — CanvasTerminal's runSmartAction is the one that catches and logs it", async () => {
		const deps = makeDeps();
		const err = new DOMException("Write permission denied.", "NotAllowedError");
		deps.copyToClipboard = vi.fn().mockRejectedValue(err);

		await expect(runSmartSelectionAction(action({ kind: "copy", parameter: "\\0" }), ctx, deps)).rejects.toBe(err);
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

	it("send_text with Press Enter: when safe asks the caller to apply its usual check", async () => {
		const deps = makeDeps();
		await runSmartSelectionAction(
			action({ kind: "send_text", parameter: "echo \\0", sendReturn: "when_safe" }),
			ctx,
			deps,
		);
		expect(deps.sendText).toHaveBeenCalledWith("echo a1b2c3d", true);
	});

	it("send_text with Press Enter: never (or unset) never asks for Enter", async () => {
		const deps = makeDeps();
		await runSmartSelectionAction(action({ kind: "send_text", parameter: "echo \\0", sendReturn: "never" }), ctx, deps);
		expect(deps.sendText).toHaveBeenCalledWith("echo a1b2c3d", false);
	});

	it("send_text does not quote: it is typed for a person to read", async () => {
		const deps = makeDeps();
		await runSmartSelectionAction(
			action({ kind: "send_text", parameter: "echo \\0", sendReturn: "when_safe" }),
			{ ...ctx, matchText: "a b" },
			deps,
		);
		expect(deps.sendText).toHaveBeenCalledWith("echo a b", true);
	});

	it("the older run_command is Send Text with Press Enter: when safe — same call, same arguments", async () => {
		for (const [parameter, over] of [
			["git show \\0", {}],
			["cd \\d && whoami # \\u@\\h", {}],
			["x \\0", { matchText: "a b", cwd: undefined }],
		] as const) {
			const older = makeDeps();
			const current = makeDeps();
			await runSmartSelectionAction(action({ kind: "run_command", parameter }), { ...ctx, ...over }, older);
			await runSmartSelectionAction(
				action({ kind: "send_text", parameter, sendReturn: "when_safe" }),
				{ ...ctx, ...over },
				current,
			);
			expect(vi.mocked(older.sendText).mock.calls).toEqual(vi.mocked(current.sendText).mock.calls);
			expect(older.sendText).toHaveBeenCalledTimes(1);
			expect(vi.mocked(older.sendText).mock.calls[0][1]).toBe(true);
		}
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

	describe('run_background (labelled "Run Command")', () => {
		const run = (parameter: string, over: Partial<SmartSelectionActionContext> = {}) => {
			const deps = makeDeps();
			return runSmartSelectionAction(action({ kind: "run_background", parameter }), { ...ctx, ...over }, deps).then(
				() => deps,
			);
		};
		const background = (deps: SmartSelectionActionDeps) => vi.mocked(deps.runBackground).mock.calls[0];

		it("runs in the background and types nothing anywhere", async () => {
			const deps = await run("imark \\0", { matchText: "docs/a.md" });
			expect(deps.runBackground).toHaveBeenCalledTimes(1);
			expect(deps.sendText).not.toHaveBeenCalled();
			expect(deps.runInNewTerminal).not.toHaveBeenCalled();
		});

		it("never puts the matched text in the command line: it is a variable reference, the value travels in env", async () => {
			const deps = await run("imark \\0", { matchText: "docs/a.md" });
			expect(background(deps)).toEqual([`imark "${shVar("TUIC_M0")}"`, "/repo", { TUIC_M0: "docs/a.md" }]);
			expect(background(deps)[0]).not.toContain("docs/a.md");
		});

		it("a hostile match changes nothing about the command line, whatever it contains", async () => {
			const evil = "x'; touch pwned; echo '$(id)`id`\"\n\\";
			const deps = await run("open \\0", { matchText: evil });
			const [command, , env] = background(deps);
			expect(command).toBe(`open "${shVar("TUIC_M0")}"`);
			expect(env).toEqual({ TUIC_M0: evil });
		});

		it("passes the terminal's folder so a relative match resolves as it would in that terminal", async () => {
			const deps = await run("imark \\0", { cwd: "/work/tree" });
			expect(background(deps)[1]).toBe("/work/tree");
		});

		it("passes an empty folder when the terminal's is unknown (the backend then uses home)", async () => {
			const deps = await run("imark \\0", { cwd: undefined });
			expect(background(deps)[1]).toBe("");
		});

		it("capture groups, the working directory, user and host travel as their own variables", async () => {
			const deps = await run("tool \\1 \\2 \\d \\u@\\h", {
				groups: ["a b", "c'd"],
				cwd: "/my dir",
				user: "j",
				host: "h",
			});
			expect(background(deps)).toEqual([
				`tool "${shVar("TUIC_M1")}" "${shVar("TUIC_M2")}" "${shVar("TUIC_D")}" "${shVar("TUIC_U")}"@"${shVar("TUIC_H")}"`,
				"/my dir",
				{ TUIC_M1: "a b", TUIC_M2: "c'd", TUIC_D: "/my dir", TUIC_U: "j", TUIC_H: "h" },
			]);
		});

		it("an unmatched capture group is an empty value, not a missing argument", async () => {
			const deps = await run("tool \\1 end", { groups: [undefined] });
			expect(background(deps)).toEqual([`tool "${shVar("TUIC_M1")}" end`, "/repo", { TUIC_M1: "" }]);
		});

		it("a template with no placeholders sends no values", async () => {
			const deps = await run("open -a Foo");
			expect(background(deps)).toEqual(["open -a Foo", "/repo", {}]);
		});

		it("quoting the placeholder yourself is safe and keeps meaning what you wrote", async () => {
			const evil = "$(touch pwned)";
			expect(background(await run('echo "<\\0>"', { matchText: evil }))).toEqual([
				`echo "<${shVar("TUIC_M0")}>"`,
				"/repo",
				{ TUIC_M0: evil },
			]);
			expect(background(await run("echo '<\\0>'", { matchText: evil }))).toEqual([
				`echo '<'"\${TUIC_M0}"'>'`,
				"/repo",
				{ TUIC_M0: evil },
			]);
		});

		it("leaves the template's own text exactly as the user wrote it (operators, quotes, \\n, \\\\)", async () => {
			const deps = await run('cd "$HOME" && tool \\0 | tee out; echo done\\nnext \\\\');
			expect(background(deps)[0]).toBe(`cd "$HOME" && tool "${shVar("TUIC_M0")}" | tee out; echo done\nnext \\`);
		});

		it("propagates a rejected runBackground — the caller reports it", async () => {
			const deps = makeDeps();
			const err = new Error("boom");
			deps.runBackground = vi.fn().mockRejectedValue(err);
			await expect(
				runSmartSelectionAction(action({ kind: "run_background", parameter: "x \\0" }), ctx, deps),
			).rejects.toBe(err);
		});

		it("Run in the background is what an absent or explicit 'background' option means", async () => {
			for (const runIn of [undefined, "background" as const]) {
				const deps = makeDeps();
				await runSmartSelectionAction(action({ kind: "run_background", parameter: "x \\0", runIn }), ctx, deps);
				expect(deps.runBackground).toHaveBeenCalledTimes(1);
				expect(deps.runInNewTerminal).not.toHaveBeenCalled();
				expect(deps.newTerminalShell).not.toHaveBeenCalled();
			}
		});

		describe("in a new terminal", () => {
			const newTerminal = (deps: SmartSelectionActionDeps, over: Partial<SmartSelectionActionContext> = {}) =>
				runSmartSelectionAction(
					action({ kind: "run_background", parameter: "imark \\0", runIn: "new_terminal" }),
					{ ...ctx, ...over },
					deps,
				);

			it("types ONE line: the value as an assignment, then /bin/sh running the script — not the match in a command", async () => {
				const deps = makeDeps();
				await newTerminal(deps, { matchText: "it's a.md" });
				expect(deps.runInNewTerminal).toHaveBeenCalledWith(`TUIC_M0='it'\\''s a.md' /bin/sh -c 'imark "\${TUIC_M0}"'`);
				expect(deps.runBackground).not.toHaveBeenCalled();
				expect(deps.sendText).not.toHaveBeenCalled();
			});

			it("asks the backend about the terminal's shell first, and types only when it is supported", async () => {
				const deps = makeDeps();
				await newTerminal(deps);
				expect(deps.newTerminalShell).toHaveBeenCalledTimes(1);
				expect(deps.runInNewTerminal).toHaveBeenCalledTimes(1);
				expect(deps.onUnsupportedNewTerminalShell).not.toHaveBeenCalled();
			});

			it("with an unsupported shell (fish, tcsh, nu, …) opens nothing, types nothing, and says which shell", async () => {
				const deps = makeDeps();
				deps.newTerminalShell = vi.fn().mockResolvedValue({ shell: "fish", supported: false });
				await newTerminal(deps, { matchText: "\\' ; touch pwned #" });
				expect(deps.runInNewTerminal).not.toHaveBeenCalled();
				expect(deps.sendText).not.toHaveBeenCalled();
				expect(deps.runBackground).not.toHaveBeenCalled();
				expect(deps.onUnsupportedNewTerminalShell).toHaveBeenCalledExactlyOnceWith("fish");
			});

			it("a working directory with a control byte is refused: nothing typed, nothing opened", async () => {
				// `\d` is percent-decoded raw from OSC 7, so a folder named with ^U can carry one; typed,
				// the line editor would discard everything before it and run the rest as a command.
				for (const cwd of ["/x\x15touch pwned; echo '", "/x\x03y", "/x\ny"]) {
					const deps = makeDeps();
					await runSmartSelectionAction(
						action({ kind: "run_background", parameter: "code \\d", runIn: "new_terminal" }),
						{ ...ctx, cwd },
						deps,
					);
					expect(deps.runInNewTerminal, JSON.stringify(cwd)).not.toHaveBeenCalled();
					expect(deps.onNewTerminalRefused).toHaveBeenCalledExactlyOnceWith("control_bytes");
				}
			});

			it("the same working directory still runs in the background, where values are environment, not typing", async () => {
				const deps = makeDeps();
				await runSmartSelectionAction(
					action({ kind: "run_background", parameter: "code \\d" }),
					{ ...ctx, cwd: "/x\x15y" },
					deps,
				);
				expect(deps.runBackground).toHaveBeenCalledTimes(1);
				expect(deps.onNewTerminalRefused).not.toHaveBeenCalled();
			});

			it("a typed line over 1000 bytes is refused, one just under it is typed", async () => {
				const tooLong = makeDeps();
				await newTerminal(tooLong, { matchText: "x".repeat(2000) });
				expect(tooLong.runInNewTerminal).not.toHaveBeenCalled();
				expect(tooLong.onNewTerminalRefused).toHaveBeenCalledExactlyOnceWith("too_long");

				// The cap counts bytes, not characters: 480 two-byte letters plus the wrapper exceed it.
				const wide = makeDeps();
				await newTerminal(wide, { matchText: "é".repeat(480) });
				expect(wide.onNewTerminalRefused).toHaveBeenCalledExactlyOnceWith("too_long");

				const fits = makeDeps();
				await newTerminal(fits, { matchText: "x".repeat(900) });
				expect(fits.runInNewTerminal).toHaveBeenCalledTimes(1);
				expect(fits.onNewTerminalRefused).not.toHaveBeenCalled();
			});

			it("an unsupported shell is reported as that, before any control-byte or length check", async () => {
				const deps = makeDeps();
				deps.newTerminalShell = vi.fn().mockResolvedValue({ shell: "fish", supported: false });
				await newTerminal(deps, { matchText: "x".repeat(2000), cwd: "/a\x15b" });
				expect(deps.onUnsupportedNewTerminalShell).toHaveBeenCalledExactlyOnceWith("fish");
				expect(deps.onNewTerminalRefused).not.toHaveBeenCalled();
			});

			it("a failing shell check is not swallowed here — the caller decides what no answer means", async () => {
				const deps = makeDeps();
				const err = new Error("no answer");
				deps.newTerminalShell = vi.fn().mockRejectedValue(err);
				await expect(newTerminal(deps)).rejects.toBe(err);
				expect(deps.runInNewTerminal).not.toHaveBeenCalled();
			});
		});

		it("the older run_command_new_terminal kind is unchanged: unquoted, no shell check, as before the merge", async () => {
			const deps = makeDeps();
			await runSmartSelectionAction(
				action({ kind: "run_command_new_terminal", parameter: "git log \\0" }),
				{ ...ctx, matchText: "a b" },
				deps,
			);
			expect(deps.runInNewTerminal).toHaveBeenCalledWith("git log a b");
			expect(deps.newTerminalShell).not.toHaveBeenCalled();
		});

		it("the older run_command (Send Text, when safe) is untouched: unquoted, typed, may auto-submit", async () => {
			const deps = makeDeps();
			await runSmartSelectionAction(
				action({ kind: "run_command", parameter: "git show \\0" }),
				{ ...ctx, matchText: "a b" },
				deps,
			);
			expect(deps.sendText).toHaveBeenCalledWith("git show a b", true);
			expect(deps.runBackground).not.toHaveBeenCalled();
		});
	});
});

describe("backgroundFailureMessage", () => {
	it("is silent for success and for a command left running", () => {
		expect(backgroundFailureMessage({ status: "succeeded" })).toBeNull();
		expect(backgroundFailureMessage({ status: "running" })).toBeNull();
	});

	it("explains 127 as a command that is not on the login shell's PATH", () => {
		const message = backgroundFailureMessage({ status: "failed", exit_code: 127 }) ?? "";
		expect(message).toContain("not found");
		expect(message).toContain("PATH");
	});

	it("explains 126 as a command that could not be run", () => {
		expect(backgroundFailureMessage({ status: "failed", exit_code: 126 })).toContain("could not be run");
	});

	it("reports a signal death", () => {
		expect(backgroundFailureMessage({ status: "failed", exit_code: null })).toBe(
			"The command was stopped by a signal.",
		);
	});

	it("reports any other code, and says why there is no output and how to get it", () => {
		const message = backgroundFailureMessage({ status: "failed", exit_code: 3 }) ?? "";
		expect(message).toContain("exited with code 3");
		expect(message).toContain("output is discarded");
		expect(message).toContain("In a new terminal");
	});
});
