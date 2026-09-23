import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
// Only `rpc` is replaced: every panel below loads its state through it on
// mount, and the placement guard only needs those loads to resolve.
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	rpc: vi.fn(),
}));

import { LocalMcpPanel } from "../../../components/SettingsPanel/tabs/services/LocalMcpPanel";
import { RemoteAccessPanel } from "../../../components/SettingsPanel/tabs/services/RemoteAccessPanel";
import { rpc } from "../../../transport";

/**
 * Placement regression guard for the Services & MCP split. The Remote Access
 * page (`RemoteAccessPanel`) owns File Access, Remote Access, Tailscale HTTPS,
 * Self-Signed HTTPS and Cloud Relay; HTTP API Server and TUIC MCP Server stay on the
 * MCP page (`LocalMcpPanel`). Moving a section between the two pages must be a
 * deliberate change to this test.
 */
describe("Remote Access / MCP — placement regression guard", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.mocked(rpc).mockImplementation((command: string) => {
			switch (command) {
				case "get_mcp_status":
					return Promise.resolve({
						enabled: true,
						running: true,
						remote_port: null,
						active_sessions: 0,
						mcp_clients: 0,
						max_sessions: 10,
						reachable: null,
						native_tools: [],
					});
				case "get_relay_status":
					return Promise.resolve({ enabled: false, connected: false, url: "", session_id: "" });
				case "load_config":
					// Remote access ON so the Self-Signed HTTPS section can render.
					return Promise.resolve({
						services: {
							server: { enabled: true, port: 9876, ipv6_enabled: false },
							auth: { username: "", password_hash: "", session_token_duration_secs: 86400, lan_auth_bypass: false },
							relay: { enabled: false, url: "", session_id: "" },
						},
					});
				case "get_local_ips":
					return Promise.resolve([]);
				// Tailscale present but without HTTPS, so the self-signed fallback shows too.
				case "get_tailscale_status":
					return Promise.resolve({ state: "NotRunning" });
				case "get_self_signed_cert_status":
					return Promise.resolve({ active: true, generated: true, not_after_unix: null, fingerprint_sha256: null });
				default:
					return Promise.resolve(undefined);
			}
		});
	});

	afterEach(() => {
		vi.clearAllTimers();
		vi.useRealTimers();
		vi.clearAllMocks();
	});

	it("the Remote Access page renders every section moved out of Services & MCP", async () => {
		const view = render(() => <RemoteAccessPanel />);
		await vi.advanceTimersByTimeAsync(0);

		for (const heading of ["File Access", "Remote Access", "Tailscale HTTPS", "Self-Signed HTTPS", "Cloud Relay"]) {
			expect(view.getByRole("heading", { name: heading })).toBeDefined();
		}
		expect(view.getByText("Enable remote access")).toBeDefined();
		expect(view.queryByRole("heading", { name: "HTTP API Server" })).toBeNull();
		expect(view.queryByRole("heading", { name: "TUIC MCP Server" })).toBeNull();

		view.unmount();
	});

	it("HTTP API Server and TUIC MCP Server stay on the MCP page", async () => {
		const view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);

		expect(view.getByRole("heading", { name: "HTTP API Server" })).toBeDefined();
		expect(view.getByRole("heading", { name: "TUIC MCP Server" })).toBeDefined();
		for (const heading of ["File Access", "Remote Access", "Cloud Relay"]) {
			expect(view.queryByRole("heading", { name: heading })).toBeNull();
		}

		view.unmount();
	});
});
