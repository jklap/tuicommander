import { render } from "@solidjs/testing-library";
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

		it("hides Collapse tools and the native tool toggles at their defaults", async () => {
			const view = await renderWith({ collapse_tools: false, disabled_native_tools: ["config", "debug"] });
			expect(collapseTools(view)).toBeNull();
			expect(nativeToolToggles(view)).toBeNull();
			// The page itself stays: only the tuning knobs hide.
			expect(view.getByText("TUIC Tools")).toBeDefined();
			view.unmount();
		});

		it("shows a control whose saved value differs from its default", async () => {
			const view = await renderWith({ collapse_tools: true, disabled_native_tools: [] });
			expect(collapseTools(view)).not.toBeNull();
			expect(nativeToolToggles(view)).not.toBeNull();
			view.unmount();
		});

		it("shows only the modified control, not the one still at its default", async () => {
			const view = await renderWith({ collapse_tools: false, disabled_native_tools: ["config"] });
			expect(collapseTools(view)).toBeNull();
			expect(nativeToolToggles(view)).not.toBeNull();
			view.unmount();
		});

		it("shows every control at its default in expert mode", async () => {
			uiStore.setSettingsExpertMode(true);
			const view = await renderWith({ collapse_tools: false, disabled_native_tools: ["config", "debug"] });
			expect(collapseTools(view)).not.toBeNull();
			expect(nativeToolToggles(view)).not.toBeNull();
			view.unmount();
		});

		// A search result for either control must land on the control itself,
		// not fall back to the section heading.
		it("scrolls to each indexed MCP expert control, not just its section", async () => {
			uiStore.setSettingsExpertMode(true);
			const view = await renderWith({ collapse_tools: false, disabled_native_tools: ["config", "debug"] });
			const entries = SETTINGS_SEARCH_INDEX.filter((entry) => entry.tab === "mcp" && entry.expert);
			expect(entries.map((entry) => entry.configKey)).toEqual(["app.collapse_tools", "app.disabled_native_tools"]);
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
