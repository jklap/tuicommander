import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../../__tests__/mocks/tauri";
import { fireEvent, render, waitFor } from "@solidjs/testing-library";

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		state: {
			ide: "vscode",
			font: "JetBrains Mono",
			defaultFontSize: 12,
			confirmBeforeQuit: true,
			confirmBeforeClosingTab: true,
		},
		setIde: vi.fn(),
		setFont: vi.fn(),
		setConfirmBeforeQuit: vi.fn(),
		setConfirmBeforeClosingTab: vi.fn(),
		isAiChatEnabled: vi.fn().mockReturnValue(false),
		isAcpConfigured: vi.fn().mockReturnValue(false),
	},
	IDE_NAMES: { vscode: "VS Code", cursor: "Cursor" },
	FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono" },
}));

vi.mock("../../../stores/notifications", () => ({
	notificationsStore: {
		state: {
			isAvailable: true,
			config: {
				enabled: true,
				volume: 0.5,
				sounds: { question: true, error: true, completion: true, warning: true },
			},
		},
		setEnabled: vi.fn(),
		setVolume: vi.fn(),
		setSoundEnabled: vi.fn(),
		testSound: vi.fn(),
		reset: vi.fn(),
	},
}));

vi.mock("../../../stores/ui", () => ({
	uiStore: { state: { settingsNavWidth: 180 }, setSettingsNavWidth: vi.fn(), persistUIPrefs: vi.fn() },
}));

vi.mock("../../../stores/repositories", () => ({
	repositoriesStore: {
		state: { repositories: {}, repoOrder: [] },
		setDisplayName: vi.fn(),
		getGroupForRepo: vi.fn(() => undefined),
		getAllReposOrdered: vi.fn(() => []),
		getConnectionId: vi.fn(() => undefined),
	},
}));

vi.mock("../../../stores/repoSettings", () => ({
	repoSettingsStore: { get: vi.fn(() => undefined), getOrCreate: vi.fn(), update: vi.fn(), reset: vi.fn() },
}));

import { settingsStore } from "../../../stores/settings";
import { SettingsPanel } from "../SettingsPanel";

const open = () => render(() => <SettingsPanel visible={true} onClose={() => {}} />);

const searchInput = (container: HTMLElement) => container.querySelector("nav input[type='text']") as HTMLInputElement;

const resultRows = (container: HTMLElement) =>
	[...container.querySelectorAll(`[class*="searchResult"] button, button[class*="searchResult"]`)] as HTMLElement[];

/** Text of an element's own text nodes — what `scrollToSetting` matches on. */
const ownText = (el: Element) =>
	[...el.childNodes]
		.filter((n) => n.nodeType === Node.TEXT_NODE)
		.map((n) => n.textContent ?? "")
		.join("")
		.trim();

/** Search, open the row naming `tab` and `row`, and resolve to the element the
 * panel scrolled to. Every candidate is spied on, so a scroll that lands on the
 * wrong element — the section instead of the control — is caught. */
async function openResult(container: HTMLElement, query: string, tab: string, row: string) {
	fireEvent.input(searchInput(container), { target: { value: query } });
	const hit = resultRows(container).find((r) => r.textContent?.includes(row) && r.textContent.includes(tab));
	expect(hit, `a "${row}" result on ${tab}`).toBeDefined();
	if (!hit) throw new Error("unreachable");
	fireEvent.click(hit);
	expect(container.querySelector("nav button[class*='active']")?.textContent).toBe(tab);

	const scrolled: Element[] = [];
	for (const el of container.querySelectorAll("h3, label, span")) {
		el.scrollIntoView = () => scrolled.push(el);
	}
	await waitFor(() => expect(scrolled).toHaveLength(1));
	return scrolled[0];
}

describe("SettingsPanel search", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.mocked(settingsStore.isAiChatEnabled).mockReturnValue(false);
	});

	afterEach(() => {
		delete (globalThis as Record<string, unknown>).__TAURI_SHIM__;
		vi.unstubAllGlobals();
	});

	it("shows no results until something is typed", () => {
		const { container } = open();
		expect(searchInput(container).value).toBe("");
		expect(resultRows(container)).toHaveLength(0);
		// The active tab is still rendered
		expect(container.textContent).toContain("Confirmations");
	});

	it("finds a setting that lives in a tab which was never opened", () => {
		const { container } = open();
		fireEvent.input(searchInput(container), { target: { value: "master volume" } });
		const rows = resultRows(container);
		expect(rows).toHaveLength(1);
		expect(rows[0].textContent).toContain("Master Volume");
		expect(rows[0].textContent).toContain("Notifications");
		// The General tab it replaced is gone while the query stands
		expect(container.textContent).not.toContain("Confirmations");
	});

	it("opens the result's tab, clears the query and scrolls to the setting", async () => {
		const { container } = open();
		fireEvent.input(searchInput(container), { target: { value: "master volume" } });
		fireEvent.click(resultRows(container)[0]);

		expect(searchInput(container).value).toBe("");
		const active = container.querySelector("nav button[class*='active']");
		expect(active?.textContent).toBe("Notifications");
		expect(container.textContent).toContain("Notification Settings");

		const setting = [...container.querySelectorAll("label")].find((el) => el.textContent === "Master Volume");
		const heading = [...container.querySelectorAll("h3")].find((h) => h.textContent === "Notification Settings");
		expect(setting).toBeDefined();
		const onSetting = vi.fn();
		const onHeading = vi.fn();
		if (setting) setting.scrollIntoView = onSetting;
		if (heading) heading.scrollIntoView = onHeading;

		// The scroll waits a frame for the new tab body to enter the document
		await waitFor(() => expect(onSetting).toHaveBeenCalled());
		// ...and it lands on the setting, not merely on the section that holds it
		expect(onHeading).not.toHaveBeenCalled();
	});

	it("restores the tab list when the query is cleared", () => {
		const { container } = open();
		fireEvent.input(searchInput(container), { target: { value: "master volume" } });
		expect(resultRows(container)).toHaveLength(1);
		fireEvent.input(searchInput(container), { target: { value: "" } });
		expect(resultRows(container)).toHaveLength(0);
		expect(container.textContent).toContain("Confirmations");
	});

	it("tells the user when nothing matches", () => {
		const { container } = open();
		fireEvent.input(searchInput(container), { target: { value: "zzzz nothing" } });
		expect(resultRows(container)).toHaveLength(0);
		expect(container.textContent).toContain("No settings match");
	});

	describe("lands on every reorganized page", () => {
		it("an upstream MCP server heading opens MCP at the upstream panel", async () => {
			const { container } = open();
			const target = await openResult(container, "upstream mcp servers", "MCP", "Upstream MCP Servers");
			expect(target.tagName).toBe("H3");
			expect(ownText(target)).toBe("Upstream MCP Servers");
		});

		it("the remote machines heading opens Remote Machines at its heading", async () => {
			const { container } = open();
			const target = await openResult(container, "remote machines", "Remote Machines", "Remote Machines");
			expect(target.tagName).toBe("H3");
			expect(ownText(target)).toBe("Remote Machines");
		});

		it("the ego executable opens AI Chat at that control", async () => {
			vi.mocked(settingsStore.isAiChatEnabled).mockReturnValue(true);
			const { container } = open();
			const target = await openResult(container, "ego executable", "AI Chat", "ego executable");
			expect(target.tagName).toBe("LABEL");
			expect(ownText(target)).toBe("ego executable");
		});

		it("a keyboard shortcut opens Keyboard Shortcuts at that control", async () => {
			const { container } = open();
			const target = await openResult(container, "global hotkey", "Keyboard Shortcuts", "Global Hotkey");
			expect(ownText(target)).toBe("Global Hotkey (Toggle Window)");
		});

		it("a terminal option opens Terminal at that control", async () => {
			const { container } = open();
			const target = await openResult(container, "cursor style", "Terminal", "Cursor Style");
			expect(ownText(target)).toBe("Cursor Style");
		});
	});

	describe("offers only what the current client can open", () => {
		it("offers nothing on AI Chat while the experimental feature is off", () => {
			const { container } = open();
			fireEvent.input(searchInput(container), { target: { value: "ego executable" } });
			expect(resultRows(container)).toHaveLength(0);
		});

		it("offers AI Chat once the feature is on", () => {
			vi.mocked(settingsStore.isAiChatEnabled).mockReturnValue(true);
			const { container } = open();
			fireEvent.input(searchInput(container), { target: { value: "ego executable" } });
			expect(resultRows(container)).toHaveLength(1);
		});

		it("offers no desktop-only control in a browser", () => {
			(globalThis as Record<string, unknown>).__TAURI_SHIM__ = true;
			// Browser mode talks HTTP; no backend listens in a unit test
			vi.stubGlobal(
				"fetch",
				vi.fn(() => Promise.resolve(new Response("{}"))),
			);
			const { container } = open();
			fireEvent.input(searchInput(container), { target: { value: "global hotkey" } });
			expect(resultRows(container)).toHaveLength(0);
		});
	});
});
