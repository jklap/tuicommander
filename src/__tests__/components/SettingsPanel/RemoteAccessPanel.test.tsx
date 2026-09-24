import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
vi.mock("../../../transport", () => ({ rpc: vi.fn() }));

import { RemoteAccessPanel } from "../../../components/SettingsPanel/tabs/services/RemoteAccessPanel";
import { rpc } from "../../../transport";

describe("RemoteAccessPanel", () => {
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
				return Promise.resolve({
					services: {
						server: { enabled: false, port: 9876, ipv6_enabled: false },
						auth: { username: "", password_hash: "", session_token_duration_secs: 86400, lan_auth_bypass: false },
						relay: { enabled: false, url: "", session_id: "" },
					},
				});
			}
			if (command === "get_local_ips") return Promise.resolve([]);
			if (command === "get_tailscale_status") return Promise.resolve({ state: "NotInstalled" });
			return Promise.resolve(undefined);
		});
	});

	afterEach(() => {
		vi.clearAllTimers();
		vi.useRealTimers();
		vi.clearAllMocks();
	});

	it("shows the remote-access content it owns", async () => {
		const view = render(() => <RemoteAccessPanel />);
		await vi.advanceTimersByTimeAsync(0);

		expect(view.getByText("Remote Access")).toBeDefined();
		expect(view.getByText("Cloud Relay")).toBeDefined();
		expect(view.getByText("Enable remote access")).toBeDefined();

		view.unmount();
	});

	it("never renders HTTP API Server or TUIC Tools content — that belongs to LocalMcpPanel", async () => {
		const view = render(() => <RemoteAccessPanel />);
		await vi.advanceTimersByTimeAsync(0);

		expect(view.queryByText("HTTP API Server")).toBeNull();
		expect(view.queryByText("TUIC Tools")).toBeNull();
		expect(view.queryByText("Server Status")).toBeNull();

		view.unmount();
	});

	it("owns a single get_mcp_status poll and clears it on unmount", async () => {
		const view = render(() => <RemoteAccessPanel />);
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
