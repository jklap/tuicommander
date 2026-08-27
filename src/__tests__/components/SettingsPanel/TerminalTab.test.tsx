import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { resetPlatformCache } from "../../../platform";

// Story 858: Shell, the terminal theme/font/cursor fields and the terminal
// display/power-management toggles moved off GeneralTab/AppearanceTab onto a
// new TerminalTab. These tests prove the move happened on both ends — present
// on the new page, gone from the old ones — and that the fields that moved
// still round-trip through the exact config key they always used.

const { mockInvoke, mockListen } = vi.hoisted(() => ({
	mockInvoke: vi.fn(),
	mockListen: vi.fn().mockResolvedValue(vi.fn()),
}));

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
