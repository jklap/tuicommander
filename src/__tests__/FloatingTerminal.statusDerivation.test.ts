import { describe, expect, it } from "vitest";
import { floatingTerminalStatusColor, floatingTerminalStatusLabel } from "../FloatingTerminal";

describe("floatingTerminalStatusColor", () => {
	it("prioritizes an error question over busy/idle/exited", () => {
		expect(floatingTerminalStatusColor("error", true, "busy")).toBe("var(--ind-terminal-error)");
	});

	it("shows the question color for a plain question, distinct from error", () => {
		expect(floatingTerminalStatusColor("question", true, "busy")).toBe("var(--ind-terminal-question)");
	});

	it("shows the busy color when isBusy is true, even with an idle shellState", () => {
		// This is the caller's contract: isBusy is expected to be sourced from
		// terminalsStore.isWorking (busy OR declaredBackgroundWork), not raw isBusy —
		// so an idle shell with declared background work must still render busy here.
		expect(floatingTerminalStatusColor(null, true, "idle")).toBe("var(--ind-terminal-busy)");
	});

	it("falls back to idle/exited/none when not busy and not awaiting input", () => {
		expect(floatingTerminalStatusColor(null, false, "idle")).toBe("var(--ind-terminal-idle)");
		expect(floatingTerminalStatusColor(null, false, "exited")).toBe("var(--ind-terminal-exited)");
		expect(floatingTerminalStatusColor(null, false, null)).toBe("var(--ind-terminal-none)");
	});
});

describe("floatingTerminalStatusLabel", () => {
	it("distinguishes question/error/awaiting-other from a busy pill", () => {
		expect(floatingTerminalStatusLabel("question", false, null)).toBe("Waiting for input");
		expect(floatingTerminalStatusLabel("error", false, null)).toBe("Error");
		expect(floatingTerminalStatusLabel("something-else", false, null)).toBe("Awaiting input");
	});

	it('labels "Running" when isBusy is true, even with an idle shellState', () => {
		expect(floatingTerminalStatusLabel(null, true, "idle")).toBe("Running");
	});

	it("falls back to Idle/Exited/empty when not busy and not awaiting input", () => {
		expect(floatingTerminalStatusLabel(null, false, "idle")).toBe("Idle");
		expect(floatingTerminalStatusLabel(null, false, "exited")).toBe("Exited");
		expect(floatingTerminalStatusLabel(null, false, null)).toBe("");
	});
});
