import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
vi.mock("../../../transport", () => ({ rpc: vi.fn() }));

import { LocalMcpPanel } from "../../../components/SettingsPanel/tabs/services/LocalMcpPanel";
import { rpc } from "../../../transport";

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
});
