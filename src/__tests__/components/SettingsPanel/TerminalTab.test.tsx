import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { resetPlatformCache } from "../../../platform";

// Story 858: Shell, the terminal theme/font/cursor fields and the terminal
// display/power-management toggles moved off GeneralTab/AppearanceTab onto a
// new TerminalTab. These tests prove the move happened on both ends — present
// on the new page, gone from the old ones — and that the fields that moved
// still round-trip through the exact config key they always used.

const { mockInvoke, mockListen, mockWriteClipboard } = vi.hoisted(() => ({
	mockInvoke: vi.fn(),
	mockListen: vi.fn().mockResolvedValue(vi.fn()),
	mockWriteClipboard: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../../utils/clipboard", () => ({ writeClipboard: mockWriteClipboard }));

vi.mock("../../../invoke", () => ({
	invoke: mockInvoke,
	listen: mockListen,
}));

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { AppearanceTab } from "../../../components/SettingsPanel/tabs/AppearanceTab";
import { GeneralTab } from "../../../components/SettingsPanel/tabs/GeneralTab";
import { TerminalTab } from "../../../components/SettingsPanel/tabs/TerminalTab";
import { settingsStore } from "../../../stores/settings";

function invokeImpl(config: Record<string, unknown> = {}) {
	return (cmd: string) => {
		if (cmd === "load_config") return Promise.resolve({ ...config });
		return Promise.resolve(undefined);
	};
}

function savedConfigs(): Record<string, unknown>[] {
	return mockInvoke.mock.calls
		.filter(([cmd]) => cmd === "save_config")
		.map(([, args]) => (args as { config: Record<string, unknown> }).config);
}

function labelExists(container: HTMLElement, label: string): boolean {
	return Array.from(container.querySelectorAll("label")).some((el) => el.textContent === label);
}

function headingExists(container: HTMLElement, heading: string): boolean {
	return Array.from(container.querySelectorAll("h3")).some((el) => el.childNodes[0]?.textContent?.trim() === heading);
}

describe("TerminalTab placement", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockImplementation(invokeImpl());
		mockListen.mockResolvedValue(vi.fn());
	});

	afterEach(() => {
		cleanup();
		vi.useRealTimers();
	});

	it("renders Shell and the terminal theme/font/cursor fields, not Power Management", async () => {
		await settingsStore.hydrate();
		const { container } = render(() => <TerminalTab />);

		expect(labelExists(container, "Shell")).toBe(true);
		expect(labelExists(container, "Terminal Theme")).toBe(true);
		expect(labelExists(container, "Terminal Font")).toBe(true);
		expect(labelExists(container, "Cursor Style")).toBe(true);
		expect(headingExists(container, "Power Management")).toBe(false);
	});

	it("renders Power Management but not Shell on GeneralTab", async () => {
		await settingsStore.hydrate();
		const { container } = render(() => <GeneralTab />);

		expect(labelExists(container, "Shell")).toBe(false);
		expect(headingExists(container, "Terminal")).toBe(false);
		expect(headingExists(container, "Power Management")).toBe(true);
	});

	it("does not render the terminal theme/font/cursor fields on AppearanceTab", async () => {
		await settingsStore.hydrate();
		const { container } = render(() => <AppearanceTab />);

		expect(labelExists(container, "Terminal Theme")).toBe(false);
		expect(labelExists(container, "Terminal Font")).toBe(false);
		expect(labelExists(container, "Cursor Style")).toBe(false);
		expect(headingExists(container, "Theme")).toBe(false);
	});

	it("persists a Shell edit into the saved config under the same key it always used", async () => {
		vi.useFakeTimers();
		mockInvoke.mockImplementation(invokeImpl({ shell: "/bin/bash" }));
		await settingsStore.hydrate();

		const { container } = render(() => <TerminalTab />);
		mockInvoke.mockClear();
		const shellInput = Array.from(container.querySelectorAll("label"))
			.find((el) => el.textContent === "Shell")
			?.parentElement?.querySelector("input");
		if (!shellInput) throw new Error("Shell input not found");
		fireEvent.input(shellInput, { target: { value: "/bin/zsh" } });

		expect(savedConfigs()).toEqual([]);
		await vi.advanceTimersByTimeAsync(600);

		expect(savedConfigs()).toHaveLength(1);
		expect(savedConfigs()[0].shell).toBe("/bin/zsh");
		expect(settingsStore.state.shell).toBe("/bin/zsh");
	});

	it("shows the link activation select with the current value and its three options", async () => {
		await settingsStore.hydrate();
		const { container } = render(() => <TerminalTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const linkSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "modifier"))!;
		expect(linkSelect.value).toBe("click");
		expect(Array.from(linkSelect.options).map((o) => o.value)).toEqual(["click", "modifier", "never"]);
	});

	it("persists a link activation change under terminal_link_activation", async () => {
		vi.useFakeTimers();
		await settingsStore.hydrate();
		const { container } = render(() => <TerminalTab />);
		mockInvoke.mockClear();
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const linkSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "modifier"))!;
		fireEvent.change(linkSelect, { target: { value: "modifier" } });
		await vi.advanceTimersByTimeAsync(600);

		expect(settingsStore.state.linkActivation).toBe("modifier");
		expect(savedConfigs()).toHaveLength(1);
		expect(savedConfigs()[0].terminal_link_activation).toBe("modifier");
	});

	function timestampModeSelect(container: HTMLElement): HTMLSelectElement {
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const select = selects.find((s) => Array.from(s.options).some((o) => o.value === "always"));
		if (!select) throw new Error("block timestamp mode select not found");
		return select;
	}

	it("shows the block timestamp mode select with the migrated value and its three options", async () => {
		mockInvoke.mockImplementation(invokeImpl({ show_block_timestamps: false }));
		await settingsStore.hydrate();
		const { container } = render(() => <TerminalTab />);
		const modeSelect = timestampModeSelect(container);
		expect(modeSelect.value).toBe("off");
		expect(Array.from(modeSelect.options).map((o) => o.value)).toEqual(["off", "modifier", "always"]);
	});

	it("persists a block timestamp mode change under block_timestamp_mode", async () => {
		vi.useFakeTimers();
		await settingsStore.hydrate();
		const { container } = render(() => <TerminalTab />);
		mockInvoke.mockClear();
		fireEvent.change(timestampModeSelect(container), { target: { value: "always" } });
		await vi.advanceTimersByTimeAsync(600);

		expect(settingsStore.state.blockTimestampMode).toBe("always");
		expect(savedConfigs().at(-1)?.block_timestamp_mode).toBe("always");
	});

	describe("shell integration snippets", () => {
		it("shows the bash and fish snippets", async () => {
			await settingsStore.hydrate();
			const { container } = render(() => <TerminalTab />);
			expect(headingExists(container, "Shell Integration")).toBe(true);
			expect(container.textContent).toContain('[ -n "$TUIC_SHELL_INTEGRATION" ] && source "$TUIC_SHELL_INTEGRATION"');
			expect(container.textContent).toContain("if set -q TUIC_SHELL_INTEGRATION; source $TUIC_SHELL_INTEGRATION; end");
		});

		it("copies the bash snippet and shows 'Copied!' feedback", async () => {
			await settingsStore.hydrate();
			const { getAllByText, findByText, unmount } = render(() => <TerminalTab />);
			fireEvent.click(getAllByText("Copy")[0]);
			expect(mockWriteClipboard).toHaveBeenCalledWith(
				'[ -n "$TUIC_SHELL_INTEGRATION" ] && source "$TUIC_SHELL_INTEGRATION"',
			);
			await findByText("Copied!");
			// The "Copied!" reset is a real setTimeout(2000) — unmount (which runs the
			// component's onCleanup) rather than let it dangle past the test.
			unmount();
		});

		it("copies the fish snippet independently of the bash one", async () => {
			await settingsStore.hydrate();
			const { getAllByText, findAllByText, unmount } = render(() => <TerminalTab />);
			fireEvent.click(getAllByText("Copy")[1]);
			expect(mockWriteClipboard).toHaveBeenCalledWith(
				"if set -q TUIC_SHELL_INTEGRATION; source $TUIC_SHELL_INTEGRATION; end",
			);
			const copiedLabels = await findAllByText("Copied!");
			expect(copiedLabels).toHaveLength(1);
			unmount();
		});
	});

	function toggleByLabel(container: HTMLElement, label: string): HTMLInputElement {
		const span = Array.from(container.querySelectorAll("span")).find((el) => el.textContent === label);
		const input = span?.parentElement?.querySelector("input[type=checkbox]");
		if (!input) throw new Error(`toggle "${label}" not found`);
		return input as HTMLInputElement;
	}

	it("renders the Session Restore section with its toggles, slider and clear button", async () => {
		await settingsStore.hydrate();
		const { container } = render(() => <TerminalTab />);

		expect(headingExists(container, "Session Restore")).toBe(true);
		expect(toggleByLabel(container, "Restore open terminals on launch").checked).toBe(true);
		expect(toggleByLabel(container, "Save terminal scrollback").checked).toBe(false);
		expect(labelExists(container, "Scrollback lines to save")).toBe(true);
		expect(container.textContent).toContain("Clear saved scrollback");
	});

	it("persists the session-restore toggles under restore_shell_terminals / restore_scrollback", async () => {
		vi.useFakeTimers();
		await settingsStore.hydrate();
		const { container } = render(() => <TerminalTab />);
		mockInvoke.mockClear();

		fireEvent.change(toggleByLabel(container, "Restore open terminals on launch"), { target: { checked: false } });
		fireEvent.change(toggleByLabel(container, "Save terminal scrollback"), { target: { checked: true } });
		await vi.advanceTimersByTimeAsync(600);

		expect(settingsStore.state.restoreShellTerminals).toBe(false);
		expect(settingsStore.state.restoreScrollback).toBe(true);
		const last = savedConfigs().at(-1);
		expect(last?.restore_shell_terminals).toBe(false);
		expect(last?.restore_scrollback).toBe(true);
	});

	it("invokes clear_saved_scrollback from the Clear saved scrollback button", async () => {
		await settingsStore.hydrate();
		const { container } = render(() => <TerminalTab />);
		const button = Array.from(container.querySelectorAll("button")).find(
			(el) => el.textContent === "Clear saved scrollback",
		);
		if (!button) throw new Error("Clear saved scrollback button not found");
		fireEvent.click(button);

		expect(mockInvoke).toHaveBeenCalledWith("clear_saved_scrollback", {});
	});

	it("persists the OSC 1337 focus/attention toggle under osc1337_focus_attention", async () => {
		vi.useFakeTimers();
		mockInvoke.mockImplementation(invokeImpl({ osc1337_focus_attention: false }));
		await settingsStore.hydrate();
		const { container } = render(() => <TerminalTab />);
		const toggle = toggleByLabel(container, "Allow terminal focus/attention requests");
		expect(toggle.checked).toBe(false);
		mockInvoke.mockClear();

		fireEvent.change(toggle, { target: { checked: true } });
		await vi.advanceTimersByTimeAsync(600);

		expect(settingsStore.state.osc1337FocusAttention).toBe(true);
		expect(savedConfigs().at(-1)?.osc1337_focus_attention).toBe(true);
	});

	describe("modifier-symbol-dependent label/hint text", () => {
		const originalPlatform = Object.getOwnPropertyDescriptor(navigator, "platform");

		afterEach(() => {
			if (originalPlatform) Object.defineProperty(navigator, "platform", originalPlatform);
			resetPlatformCache();
		});

		function setPlatform(value: string) {
			Object.defineProperty(navigator, "platform", { value, configurable: true });
			// The component reads isMacOS(), which memoizes detectPlatform() —
			// without this, whichever platform an earlier test in this file (or an
			// earlier file in the same worker) set first sticks for every render.
			resetPlatformCache();
		}

		it("labels the modifier option ⌘Click and says Cmd in the hint on macOS", () => {
			setPlatform("MacIntel");
			const { container, getByText } = render(() => <TerminalTab />);
			const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
			const linkSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "modifier"))!;
			const modifierOption = Array.from(linkSelect.options).find((o) => o.value === "modifier")!;

			expect(modifierOption.textContent).toBe("⌘Click");
			expect(getByText(/only while Cmd is held/)).toBeTruthy();
		});

		it("labels the modifier option Ctrl+Click and says Ctrl in the hint off macOS", () => {
			setPlatform("Win32");
			const { container, getByText } = render(() => <TerminalTab />);
			const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
			const linkSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "modifier"))!;
			const modifierOption = Array.from(linkSelect.options).find((o) => o.value === "modifier")!;

			expect(modifierOption.textContent).toBe("Ctrl+Click");
			expect(getByText(/only while Ctrl is held/)).toBeTruthy();
		});
	});

	describe("modifier-symbol-dependent label/hint text", () => {
		const originalPlatform = Object.getOwnPropertyDescriptor(navigator, "platform");

		afterEach(() => {
			if (originalPlatform) Object.defineProperty(navigator, "platform", originalPlatform);
			resetPlatformCache();
		});

		function setPlatform(value: string) {
			Object.defineProperty(navigator, "platform", { value, configurable: true });
			// The component reads isMacOS(), which memoizes detectPlatform() —
			// without this, whichever platform an earlier test in this file (or an
			// earlier file in the same worker) set first sticks for every render.
			resetPlatformCache();
		}

		it("labels the modifier option ⌘Click and says Cmd in the hint on macOS", async () => {
			setPlatform("MacIntel");
			await settingsStore.hydrate();
			const { container, getByText } = render(() => <TerminalTab />);
			const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
			const linkSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "modifier"))!;
			const modifierOption = Array.from(linkSelect.options).find((o) => o.value === "modifier")!;

			expect(modifierOption.textContent).toBe("⌘Click");
			expect(getByText(/only while Cmd is held/)).toBeTruthy();
		});

		it("labels the modifier option Ctrl+Click and says Ctrl in the hint off macOS", async () => {
			setPlatform("Win32");
			await settingsStore.hydrate();
			const { container, getByText } = render(() => <TerminalTab />);
			const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
			const linkSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "modifier"))!;
			const modifierOption = Array.from(linkSelect.options).find((o) => o.value === "modifier")!;

			expect(modifierOption.textContent).toBe("Ctrl+Click");
			expect(getByText(/only while Ctrl is held/)).toBeTruthy();
		});
	});
});
