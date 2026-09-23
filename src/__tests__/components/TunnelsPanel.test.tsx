import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { TunnelsPanel } from "../../components/TunnelsPanel/TunnelsPanel";
import { tunnelPanelStore } from "../../stores/tunnelPanel";
import type { ActiveTunnel, TunnelProfile } from "../../stores/tunnels";
import { tunnelsStore } from "../../stores/tunnels";
import { mockInvoke } from "../mocks/tauri";

function makeProfile(overrides: Partial<TunnelProfile> = {}): TunnelProfile {
	return {
		id: "p1",
		name: "dev tunnel",
		host: "example.test",
		port: 22,
		user: "boss",
		identity_file: null,
		forwards: [],
		options: { server_alive_interval: 15, server_alive_count_max: 3, strict_host_key_checking: "AcceptNew" },
		auto_connect: false,
		...overrides,
	};
}

async function flushMicrotasks(): Promise<void> {
	await new Promise<void>((resolve) => setImmediate(resolve));
}

describe("TunnelsPanel", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_tunnel_profiles") return Promise.resolve([]);
			if (cmd === "list_active_tunnels") return Promise.resolve([]);
			if (cmd === "list_ssh_config_hosts") return Promise.resolve([]);
			if (cmd === "list_ssh_agent_keys") return Promise.resolve({ keys: [], agent_type: "" });
			return Promise.resolve(undefined);
		});
		tunnelPanelStore.close();
	});

	afterEach(async () => {
		cleanup();
		tunnelPanelStore.close();
		await flushMicrotasks();
	});

	it("renders nothing while the panel is closed", async () => {
		const { queryByText } = render(() => <TunnelsPanel />);
		await flushMicrotasks();
		expect(queryByText("SSH Tunnels")).toBeNull();
	});

	it("shows the empty state when there are no profiles", async () => {
		tunnelPanelStore.open();
		const { getByText } = render(() => <TunnelsPanel />);
		await flushMicrotasks();
		expect(getByText(/No tunnel profiles yet/)).toBeTruthy();
	});

	it("renders a profile row with its meta line and status badge", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_tunnel_profiles") return Promise.resolve([makeProfile()]);
			if (cmd === "list_active_tunnels")
				return Promise.resolve([{ id: "p1", status: { type: "connected" }, started_at: "t" } satisfies ActiveTunnel]);
			return Promise.resolve(undefined);
		});
		await tunnelsStore.refreshProfiles();
		await tunnelsStore.refreshActiveTunnels();

		tunnelPanelStore.open();
		const { getByText } = render(() => <TunnelsPanel />);
		await flushMicrotasks();

		expect(getByText("dev tunnel")).toBeTruthy();
		expect(getByText("boss@example.test:22")).toBeTruthy();
		expect(getByText("connected")).toBeTruthy();
		expect(getByText("Stop")).toBeTruthy(); // running → toggle button reads "Stop"
	});

	it("Start button calls tunnelsStore.startTunnel for a stopped profile", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_tunnel_profiles") return Promise.resolve([makeProfile({ id: "p2" })]);
			if (cmd === "list_active_tunnels") return Promise.resolve([]);
			if (cmd === "start_tunnel") return Promise.resolve();
			if (cmd === "get_tunnel_status")
				return Promise.resolve({ id: "p2", status: { type: "connected" }, started_at: "t" });
			return Promise.resolve(undefined);
		});
		await tunnelsStore.refreshProfiles();
		await tunnelsStore.refreshActiveTunnels();

		vi.useFakeTimers();
		tunnelPanelStore.open();
		const { getByText } = render(() => <TunnelsPanel />);
		await vi.advanceTimersByTimeAsync(0);

		fireEvent.click(getByText("Start"));
		await vi.advanceTimersByTimeAsync(2000);

		expect(mockInvoke).toHaveBeenCalledWith("start_tunnel", { id: "p2" });
		vi.useRealTimers();
	});

	it("Stop button calls tunnelsStore.stopTunnel for a running profile", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_tunnel_profiles") return Promise.resolve([makeProfile({ id: "p3" })]);
			if (cmd === "list_active_tunnels")
				return Promise.resolve([{ id: "p3", status: { type: "connected" }, started_at: "t" } satisfies ActiveTunnel]);
			if (cmd === "stop_tunnel") return Promise.resolve();
			return Promise.resolve(undefined);
		});
		await tunnelsStore.refreshProfiles();
		await tunnelsStore.refreshActiveTunnels();

		tunnelPanelStore.open();
		const { getByText } = render(() => <TunnelsPanel />);
		await flushMicrotasks();

		fireEvent.click(getByText("Stop"));
		await flushMicrotasks();

		expect(mockInvoke).toHaveBeenCalledWith("stop_tunnel", { id: "p3" });
	});

	it("Del button calls tunnelsStore.deleteProfile and refreshes the list", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_tunnel_profiles") return Promise.resolve([makeProfile({ id: "p4" })]);
			if (cmd === "list_active_tunnels") return Promise.resolve([]);
			if (cmd === "delete_tunnel_profile") return Promise.resolve();
			return Promise.resolve(undefined);
		});
		await tunnelsStore.refreshProfiles();
		await tunnelsStore.refreshActiveTunnels();

		tunnelPanelStore.open();
		const { getByText } = render(() => <TunnelsPanel />);
		await flushMicrotasks();

		fireEvent.click(getByText("Del"));
		await flushMicrotasks();

		expect(mockInvoke).toHaveBeenCalledWith("delete_tunnel_profile", { id: "p4" });
		expect(mockInvoke).toHaveBeenCalledWith("list_tunnel_profiles");
	});

	it("Log button fetches and renders audit entries; Hide clears them", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_tunnel_profiles") return Promise.resolve([makeProfile({ id: "p5" })]);
			if (cmd === "list_active_tunnels") return Promise.resolve([]);
			if (cmd === "get_tunnel_audit")
				return Promise.resolve([
					{ tunnel_id: "p5", timestamp: "2026-01-01T00:00:00Z", kind: "started", message: null },
				]);
			return Promise.resolve(undefined);
		});
		await tunnelsStore.refreshProfiles();
		await tunnelsStore.refreshActiveTunnels();

		tunnelPanelStore.open();
		const { getByText, queryByText } = render(() => <TunnelsPanel />);
		await flushMicrotasks();

		fireEvent.click(getByText("Log"));
		await flushMicrotasks();

		expect(mockInvoke).toHaveBeenCalledWith("get_tunnel_audit", { id: "p5", limit: 20 });
		expect(getByText("started")).toBeTruthy();

		fireEvent.click(getByText("Hide"));
		await flushMicrotasks();
		expect(queryByText("started")).toBeNull();
	});

	it("+ New Tunnel opens the editor modal", async () => {
		tunnelPanelStore.open();
		const { getByText } = render(() => <TunnelsPanel />);
		await flushMicrotasks();

		fireEvent.click(getByText("+ New Tunnel"));
		await flushMicrotasks();

		expect(getByText("New Tunnel")).toBeTruthy();
	});
});
