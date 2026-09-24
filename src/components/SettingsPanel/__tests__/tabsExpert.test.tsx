import { cleanup, render, waitFor } from "@solidjs/testing-library";
import type { Component } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Every expert control on General, Appearance, Notifications and Terminal:
// hidden in basic mode while its saved value equals the Rust default, shown
// once the saved value differs, and shown in expert mode. Values reach the
// control through the real store hydration, so a wrong configKey or a value
// in the wrong shape keeps the control visible and fails the "hidden" case.

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock("../../../invoke", () => ({
	invoke: mockInvoke,
	listen: vi.fn().mockResolvedValue(vi.fn()),
}));

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// The real uiStore persists the expert pref through a debounced timer.
vi.mock("../../../stores/ui", async () => {
	const { createStore } = await import("solid-js/store");
	const [state, setState] = createStore({ settingsExpertMode: false });
	return {
		uiStore: {
			state,
			setSettingsExpertMode: (enabled: boolean) => setState("settingsExpertMode", enabled),
			resetLayout: vi.fn(),
		},
	};
});

// The audio output picker renders only on desktop.
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	isTauri: () => true,
}));

import { notificationsStore } from "../../../stores/notifications";
import { settingsStore } from "../../../stores/settings";
import { settingsExpertStore } from "../../../stores/settingsExpert";
import { uiStore } from "../../../stores/ui";
import { AppearanceTab } from "../tabs/AppearanceTab";
import { GeneralTab } from "../tabs/GeneralTab";
import { NotificationsTab } from "../tabs/NotificationsTab";
import { TerminalTab } from "../tabs/TerminalTab";

/** The Rust `impl Default` values (config.rs) of the fields under test. */
const APP_DEFAULTS = {
	standby_timeout_minutes: 5,
	index_strategy: "active_and_switch",
	update_channel: "stable",
	split_tab_mode: "separate",
	tab_ordering_mode: "grouped-by-type",
	tab_cycling_all_types: false,
	max_tab_name_length: 25,
	font_weight: 400,
	osc52_clipboard: true,
	block_folding_enabled: true,
	show_scrollbar_marks: true,
	scrollback_reflow: true,
};
const NOTIFICATION_DEFAULTS = {
	enabled: true,
	volume: 0.5,
	// Absent from the serialized Rust default (`skip_serializing_if`); listed
	// here so the test exercises the wrapping rather than that gap.
	audio_device: null,
	silence_remote_completions: true,
	toasts_in_bell: true,
};

interface Case {
	page: string;
	tab: Component;
	label: string;
	domain: "app" | "notifications";
	field: string;
	modified: unknown;
}

const CASES: Case[] = [
	{
		page: "General",
		tab: GeneralTab,
		label: "Auto-Standby Timeout",
		domain: "app",
		field: "standby_timeout_minutes",
		modified: 0,
	},
	{
		page: "General",
		tab: GeneralTab,
		label: "Content Indexing",
		domain: "app",
		field: "index_strategy",
		modified: "disabled",
	},
	{
		page: "General",
		tab: GeneralTab,
		label: "Update Channel",
		domain: "app",
		field: "update_channel",
		modified: "nightly",
	},
	{
		page: "Appearance",
		tab: AppearanceTab,
		label: "Split Tab Mode",
		domain: "app",
		field: "split_tab_mode",
		modified: "unified",
	},
	{
		page: "Appearance",
		tab: AppearanceTab,
		label: "Tab Ordering",
		domain: "app",
		field: "tab_ordering_mode",
		modified: "free",
	},
	{
		page: "Appearance",
		tab: AppearanceTab,
		label: "Cycle All Tab Types",
		domain: "app",
		field: "tab_cycling_all_types",
		modified: true,
	},
	{
		page: "Appearance",
		tab: AppearanceTab,
		label: "Max Tab Name Length",
		domain: "app",
		field: "max_tab_name_length",
		modified: 40,
	},
	{ page: "Terminal", tab: TerminalTab, label: "Font Weight", domain: "app", field: "font_weight", modified: 300 },
	{
		page: "Terminal",
		tab: TerminalTab,
		label: "Allow OSC 52 clipboard writes",
		domain: "app",
		field: "osc52_clipboard",
		modified: false,
	},
	{
		page: "Terminal",
		tab: TerminalTab,
		label: "Block folding",
		domain: "app",
		field: "block_folding_enabled",
		modified: false,
	},
	{
		page: "Terminal",
		tab: TerminalTab,
		label: "Show scrollbar marks",
		domain: "app",
		field: "show_scrollbar_marks",
		modified: false,
	},
	{
		page: "Terminal",
		tab: TerminalTab,
		label: "Reflow scrollback on resize",
		domain: "app",
		field: "scrollback_reflow",
		modified: false,
	},
	{
		page: "Notifications",
		tab: NotificationsTab,
		label: "Audio Output Device",
		domain: "notifications",
		field: "audio_device",
		modified: "USB Speakers",
	},
	{
		page: "Notifications",
		tab: NotificationsTab,
		label: "Silence completions from MCP sessions",
		domain: "notifications",
		field: "silence_remote_completions",
		modified: false,
	},
	{
		page: "Notifications",
		tab: NotificationsTab,
		label: "Keep toasts in the bell",
		domain: "notifications",
		field: "toasts_in_bell",
		modified: false,
	},
];

/** Hydrate the real stores from these saved configs and load the defaults. */
async function setup(app: Record<string, unknown>, notifications: Record<string, unknown>) {
	mockInvoke.mockImplementation((cmd: string) => {
		if (cmd === "load_config") return Promise.resolve({ ...APP_DEFAULTS, ...app });
		if (cmd === "load_notification_config") return Promise.resolve({ ...NOTIFICATION_DEFAULTS, ...notifications });
		if (cmd === "get_config_defaults")
			return Promise.resolve({ app: APP_DEFAULTS, notifications: NOTIFICATION_DEFAULTS, agent_settings: {} });
		return Promise.resolve(undefined);
	});
	await settingsStore.hydrate();
	await notificationsStore.hydrate();
	await settingsExpertStore.open();
}

const hasText = (container: HTMLElement, text: string) =>
	[...container.querySelectorAll("label, span")].some((el) => el.textContent === text);

describe("expert controls on General, Appearance, Notifications and Terminal", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		settingsExpertStore._resetForTests();
		uiStore.setSettingsExpertMode(false);
	});

	afterEach(() => cleanup());

	describe.each(CASES)("$page — $label", ({ tab: Tab, label, domain, field, modified }) => {
		const override = (value: unknown) => (domain === "app" ? [{ [field]: value }, {}] : [{}, { [field]: value }]);

		it("is hidden in basic mode at its default", async () => {
			await setup({}, {});
			const { container } = render(() => <Tab />);
			// Guard against a vacuous pass: the page itself rendered.
			await waitFor(() => expect(container.querySelector("h3")).not.toBeNull());
			expect(hasText(container, label)).toBe(false);
		});

		it("is shown in basic mode once modified", async () => {
			const [app, notifications] = override(modified);
			await setup(app, notifications);
			const { container } = render(() => <Tab />);
			expect(hasText(container, label)).toBe(true);
		});

		it("is shown in expert mode at its default", async () => {
			await setup({}, {});
			uiStore.setSettingsExpertMode(true);
			const { container } = render(() => <Tab />);
			expect(hasText(container, label)).toBe(true);
		});
	});
});
