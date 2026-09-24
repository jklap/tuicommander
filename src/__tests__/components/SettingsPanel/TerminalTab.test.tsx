import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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
});
