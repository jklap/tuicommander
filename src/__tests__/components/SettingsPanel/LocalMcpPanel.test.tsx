import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
// Only `rpc` is replaced: `invoke` (the config defaults) needs the real
// `isTauri` and connection resolver to reach the Tauri mock.
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	rpc: vi.fn(),
}));
vi.mock("../../../stores/ui", async () => {
	const { createStore } = await import("solid-js/store");
	const [state, setState] = createStore({ settingsExpertMode: false });
	return {
		uiStore: {
			state,
			setSettingsExpertMode: vi.fn((enabled: boolean) => setState("settingsExpertMode", enabled)),
		},
	};
});

import { scrollToSetting } from "../../../components/SettingsPanel/SettingsSearch";
import { SETTINGS_SEARCH_INDEX } from "../../../components/SettingsPanel/settingsSearchIndex";
import { LocalMcpPanel } from "../../../components/SettingsPanel/tabs/services/LocalMcpPanel";
import { settingsExpertStore } from "../../../stores/settingsExpert";
import { uiStore } from "../../../stores/ui";
import { rpc } from "../../../transport";
import { mockInvoke } from "../../mocks/tauri";

const nativeTools = ["session", "plugin_dev_guide", "config", "debug", "remote", "secret", "progress", "telegram"].map(
	(name) => ({ name, summary: `${name} summary`, description: `${name} full description` }),
);

describe("LocalMcpPanel", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.mocked(rpc).mockImplementation((command: string) => {
			if (command === "get_mcp_status") {
				return Promise.resolve({
					enabled: true,
					running: true,
					remote_port: null,
					active_sessions: 0,
					mcp_clients: 0,
					max_sessions: 10,
					reachable: null,
					native_tools: nativeTools,
				});
			}
			if (command === "get_relay_status") {
				return Promise.resolve({ enabled: false, connected: false, url: "", session_id: "" });
			}
			if (command === "load_config") {
				return Promise.resolve({ disabled_native_tools: [], collapse_tools: false });
			}
			return Promise.resolve(undefined);
		});
	});

	// Catches: a static UI inventory silently drops a newly registered backend tool.
	it("renders every backend native tool, including new tools and full descriptions", async () => {
		const view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		for (const tool of nativeTools) {
			expect(view.getByRole("checkbox", { name: tool.name })).toBeDefined();
			expect(view.getByText(tool.summary)).toBeDefined();
			expect(view.getByText(tool.description)).toBeDefined();
		}
		view.unmount();
	});

	// Catches: disabled tools disappear from Settings and can never be re-enabled.
	it("re-enables a disabled backend tool without dropping other disabled names", async () => {
		const base = vi.mocked(rpc).getMockImplementation();
		vi.mocked(rpc).mockImplementation((command: string, args?: Record<string, unknown>) =>
			command === "load_config"
				? Promise.resolve({ disabled_native_tools: ["telegram", "debug", "unknown_tool"], collapse_tools: false })
				: (base?.(command, args) as Promise<never>),
		);
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "load_config" ? { disabled_native_tools: ["telegram", "debug", "unknown_tool"] } : undefined,
			),
		);
		const view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		const toggle = view.getByRole("checkbox", { name: "telegram" }) as HTMLInputElement;
		expect(toggle.checked).toBe(false);
		fireEvent.click(toggle);
		await vi.advanceTimersByTimeAsync(0);
		expect(mockInvoke).toHaveBeenCalledWith(
			"save_config",
			expect.objectContaining({
				config: { disabled_native_tools: ["debug", "unknown_tool"] },
			}),
		);
		fireEvent.click(toggle);
		await vi.advanceTimersByTimeAsync(0);
		expect(mockInvoke).toHaveBeenLastCalledWith(
			"save_config",
			expect.objectContaining({
				config: { disabled_native_tools: ["debug", "unknown_tool", "telegram"] },
			}),
		);
		view.unmount();
	});

	afterEach(() => {
		settingsExpertStore._resetForTests();
		uiStore.setSettingsExpertMode(false);
		vi.clearAllTimers();
		vi.useRealTimers();
		vi.clearAllMocks();
	});

	it("shows the local MCP content it owns", async () => {
		const view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);

		expect(view.getByText("HTTP API Server")).toBeDefined();
		expect(view.getByText("TUIC Tools")).toBeDefined();
		expect(view.getByText("Server Status")).toBeDefined();

		view.unmount();
	});

	it("never renders Remote Access or Cloud Relay content — that belongs to RemoteAccessPanel", async () => {
		const view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);

		expect(view.queryByText("Remote Access")).toBeNull();
		expect(view.queryByText("Cloud Relay")).toBeNull();
		expect(view.queryByText("Enable remote access")).toBeNull();

		view.unmount();
	});

	it("owns a single get_mcp_status poll and clears it on unmount", async () => {
		const view = render(() => <LocalMcpPanel />);
		const statusCalls = () => vi.mocked(rpc).mock.calls.filter(([command]) => command === "get_mcp_status").length;

		await vi.advanceTimersByTimeAsync(0);
		expect(statusCalls()).toBe(1);

		await vi.advanceTimersByTimeAsync(3000);
		expect(statusCalls()).toBe(2);

		view.unmount();
		await vi.advanceTimersByTimeAsync(6000);
		expect(statusCalls()).toBe(2);
	});

	describe("expert controls", () => {
		const DEFAULTS = {
			app: { collapse_tools: false, disabled_native_tools: ["config", "debug"] },
			notifications: {},
			agent_settings: {},
		};

		/** Render the page over a saved config, with the Rust defaults loaded. */
		async function renderWith(saved: { collapse_tools: boolean; disabled_native_tools: string[] }) {
			mockInvoke.mockImplementation((cmd: string) =>
				cmd === "get_config_defaults" ? Promise.resolve(DEFAULTS) : Promise.resolve(undefined),
			);
			const base = vi.mocked(rpc).getMockImplementation();
			vi.mocked(rpc).mockImplementation((command: string, args?: Record<string, unknown>) => {
				if (command === "load_config") return Promise.resolve(saved);
				return base?.(command, args) as Promise<never>;
			});
			await settingsExpertStore.open();
			const view = render(() => <LocalMcpPanel />);
			await vi.advanceTimersByTimeAsync(0);
			return view;
		}

		const collapseTools = (view: ReturnType<typeof render>) => view.queryByText(/Collapse tools/);
		const nativeToolToggles = (view: ReturnType<typeof render>) => view.queryByText("plugin_dev_guide");

		it("hides Collapse tools at its default, but keeps the basic native tool toggles", async () => {
			const view = await renderWith({ collapse_tools: false, disabled_native_tools: ["config", "debug"] });
			expect(collapseTools(view)).toBeNull();
			expect(nativeToolToggles(view)).not.toBeNull();
			expect(view.getByText("Native tools")).toBeDefined();
			view.unmount();
		});

		it("shows Collapse tools once its saved value differs from the default", async () => {
			const view = await renderWith({ collapse_tools: true, disabled_native_tools: ["config", "debug"] });
			expect(collapseTools(view)).not.toBeNull();
			view.unmount();
		});

		it("shows Collapse tools at its default in expert mode", async () => {
			uiStore.setSettingsExpertMode(true);
			const view = await renderWith({ collapse_tools: false, disabled_native_tools: ["config", "debug"] });
			expect(collapseTools(view)).not.toBeNull();
			view.unmount();
		});

		// The toggles render the saved list, so they must not show the `[]`
		// placeholder (every tool on) before `load_config` answers.
		it("renders the native tool toggles only once the saved list has loaded", async () => {
			let answer: (config: unknown) => void = () => {};
			const base = vi.mocked(rpc).getMockImplementation();
			vi.mocked(rpc).mockImplementation((command: string, args?: Record<string, unknown>) =>
				command === "load_config"
					? new Promise((resolve) => {
							answer = resolve;
						})
					: (base?.(command, args) as Promise<never>),
			);
			const view = render(() => <LocalMcpPanel />);
			await vi.advanceTimersByTimeAsync(0);
			expect(nativeToolToggles(view)).toBeNull();

			answer({ collapse_tools: false, disabled_native_tools: ["config", "debug"] });
			await vi.advanceTimersByTimeAsync(0);
			const configToggle = view.getByText("config").closest("div")?.parentElement?.querySelector("input");
			expect(configToggle?.checked).toBe(false);
			view.unmount();
		});

		// A search result for either TUIC Tools control must land on the control itself,
		// not fall back to the section heading.
		it("scrolls to each indexed TUIC Tools control, not just its section", async () => {
			uiStore.setSettingsExpertMode(true);
			const view = await renderWith({ collapse_tools: false, disabled_native_tools: ["config", "debug"] });
			const entries = SETTINGS_SEARCH_INDEX.filter(
				(entry) => entry.tab === "mcp" && entry.section === "TUIC Tools" && entry.label,
			);
			expect(entries.map((entry) => entry.label)).toEqual([
				"Collapse tools — Speakeasy MCP (reduces AI context ~98%)",
				"Native tools",
			]);
			for (const entry of entries) {
				const scrolled: Element[] = [];
				for (const el of view.container.querySelectorAll("h3, label, span")) {
					(el as HTMLElement).scrollIntoView = () => scrolled.push(el);
				}
				expect(scrollToSetting(view.container, entry.section, entry.label)).toBe(true);
				expect(scrolled.map((el) => el.textContent?.trim())).toEqual([entry.label]);
			}
			view.unmount();
		});
	});
});
