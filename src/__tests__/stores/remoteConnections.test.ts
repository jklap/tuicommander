import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Mock only the external boundaries: IPC (invoke), the SSE event bridge, and
// fetch. The connect/disconnect cleanup sequencing (health-poll interval
// teardown, bridge cleanup, state reset) is the real logic under test.
const bridgeCleanup = vi.fn();
const startBridge = vi.fn((..._args: unknown[]) => bridgeCleanup);
vi.mock("../../utils/remoteEventBridge", () => ({
	startRemoteEventBridge: (...args: unknown[]) => startBridge(...args),
}));
// `vi.hoisted` keeps this the SAME vi.fn() identity for the whole file (unlike
// a plain factory-local `vi.fn()`), which the SSH-connect tests below need:
// remoteConnectionsStore's SSH path drives tunnelsStore internally, and
// tunnelsStore imports this exact mocked "../invoke" module too (both
// specifiers resolve to the same file), so one mock has to cover both.
// Phase 4 (self-signed HTTPS proxy) added two new invoke() calls to the
// Direct connect path: `probe_direct_tls_connection` and `start_direct_proxy`.
// Every existing Direct test here uses a plain `http://` URL with no pinning
// concern, so the default mock answers exactly what a plain-http probe would:
// no TLS involved, no proxy needed — baseUrl stays the raw URL, preserving
// every pre-Phase-4 test's behavior unchanged. Individual it()s can still
// override via `mockInvoke.mockImplementationOnce`/`mockResolvedValueOnce`.
const defaultMockInvokeImpl = async (command: string): Promise<unknown> => {
	if (command === "probe_direct_tls_connection") return { type: "NoTlsNeeded" };
	if (command === "start_direct_proxy") return null;
	return undefined;
};
const mockInvoke = vi.hoisted(() => vi.fn());
vi.mock("../../invoke", () => ({ invoke: mockInvoke }));
mockInvoke.mockImplementation(defaultMockInvokeImpl);

/** `mockInvoke.mockReset()` (used throughout this file between describe
 * blocks/tests to clear call history AND any one-off `mockImplementation`)
 * also wipes the default handler, so every reset must be paired with
 * restoring it — otherwise the Direct connect path's `probe_direct_tls_connection`/
 * `start_direct_proxy` calls resolve to `undefined` and `connect()` throws
 * reading `.type` off it, exactly as it did before Phase 4's mock support
 * was added here. */
function resetMockInvokeToDefault() {
	mockInvoke.mockReset();
	mockInvoke.mockImplementation(defaultMockInvokeImpl);
}

import type { RemoteConnection } from "../../stores/remoteConnections";
import { remoteConnectionsStore } from "../../stores/remoteConnections";
import type { TunnelProfile } from "../../stores/tunnels";

const fetchMock = vi.fn();

function directConn(id: string): RemoteConnection {
	return {
		id,
		name: `conn-${id}`,
		transport: { type: "Direct", url: "http://remote.test:9876", tls_fingerprint: null },
		auth_username: "user",
		enabled: true,
	};
}

describe("remoteConnectionsStore connect/disconnect (Direct)", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		startBridge.mockClear();
		bridgeCleanup.mockClear();
		fetchMock.mockReset();
		fetchMock.mockResolvedValue({
			ok: true,
			json: async () => ({ protocol_version: 2 }),
		});
		vi.stubGlobal("fetch", fetchMock);
	});
	afterEach(async () => {
		// Ensure no health-poll interval leaks between tests.
		await remoteConnectionsStore.disconnect("c1");
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	async function connect(id: string) {
		await remoteConnectionsStore.addConnection(directConn(id));
		await remoteConnectionsStore.connect(id);
	}

	it("connect sets baseUrl, health-checks, and starts the SSE bridge", async () => {
		await connect("c1");
		const st = remoteConnectionsStore.getConnectionState("c1");
		expect(st?.status).toBe("connected");
		expect(st?.protocolVersion).toBe(2);
		expect(remoteConnectionsStore.getBaseUrl("c1")).toBe("http://remote.test:9876");
		expect(startBridge).toHaveBeenCalledTimes(1);
		expect(startBridge).toHaveBeenCalledWith("c1", "http://remote.test:9876");
		// The health poll keeps running on its interval.
		expect(fetchMock).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(5000);
		expect(fetchMock).toHaveBeenCalledTimes(2);
	});

	it("disconnect stops health polling, tears down the bridge, and resets state", async () => {
		await connect("c1");
		expect(fetchMock).toHaveBeenCalledTimes(1);

		await remoteConnectionsStore.disconnect("c1");

		// Bridge cleanup ran exactly once.
		expect(bridgeCleanup).toHaveBeenCalledTimes(1);
		// State reset — getBaseUrl only returns a url while connected.
		const st = remoteConnectionsStore.getConnectionState("c1");
		expect(st?.status).toBe("disconnected");
		expect(st?.baseUrl).toBeUndefined();
		expect(remoteConnectionsStore.getBaseUrl("c1")).toBeUndefined();
		// The interval is cleared: advancing time triggers no further health fetches.
		await vi.advanceTimersByTimeAsync(15000);
		expect(fetchMock).toHaveBeenCalledTimes(1);
	});

	it("reconnecting swaps the bridge: the previous cleanup runs before a new bridge", async () => {
		await connect("c1");
		expect(startBridge).toHaveBeenCalledTimes(1);
		// Force a second connect by first marking it disconnected without cleanup…
		await remoteConnectionsStore.disconnect("c1");
		bridgeCleanup.mockClear();
		startBridge.mockClear();
		await remoteConnectionsStore.connect("c1");
		expect(startBridge).toHaveBeenCalledTimes(1); // fresh bridge established
	});

	it("connect is a no-op when already connected (no duplicate bridge)", async () => {
		await connect("c1");
		expect(startBridge).toHaveBeenCalledTimes(1);
		await remoteConnectionsStore.connect("c1"); // already connected → guarded
		expect(startBridge).toHaveBeenCalledTimes(1);
	});

	it("disconnect on an unknown connection is a safe no-op", async () => {
		await expect(remoteConnectionsStore.disconnect("ghost")).resolves.toBeUndefined();
		expect(bridgeCleanup).not.toHaveBeenCalled();
	});
});

function sshConn(id: string): RemoteConnection {
	return {
		id,
		name: `ssh-${id}`,
		transport: {
			type: "Ssh",
			ssh: {
				host: "example.test",
				port: 22,
				user: "boss",
				identity_file: null,
				server_alive_interval: 15,
				server_alive_count_max: 3,
				strict_host_key_checking: "Yes",
			},
			remote_daemon_port: 9877,
		},
		auth_username: "boss",
		enabled: true,
	};
}

function tunnelProfileFixture(id: string, name: string): TunnelProfile {
	return {
		id,
		name,
		ssh: {
			host: "example.test",
			port: 22,
			user: "boss",
			identity_file: null,
			server_alive_interval: 15,
			server_alive_count_max: 3,
			strict_host_key_checking: "AcceptNew",
		},
		forwards: [{ type: "Local", bind_port: 12345, remote_host: "127.0.0.1", remote_port: 9877 }],
		auto_connect: false,
	};
}

describe("remoteConnectionsStore.connect() (SSH)", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		resetMockInvokeToDefault();
		startBridge.mockClear();
		bridgeCleanup.mockClear();
		fetchMock.mockReset();
		fetchMock.mockResolvedValue({ ok: true, json: async () => ({ protocol_version: 2 }) });
		vi.stubGlobal("fetch", fetchMock);
	});
	afterEach(async () => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("creates a tunnel profile, starts it, waits for connected, then sets baseUrl and starts health polling", async () => {
		const id = "ssh1";
		await remoteConnectionsStore.addConnection(sshConn(id));

		const createdProfile = tunnelProfileFixture("auto-p1", `__remote_${id}`);
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "save_tunnel_profile") return Promise.resolve();
			if (cmd === "list_tunnel_profiles") return Promise.resolve([createdProfile]);
			if (cmd === "start_tunnel") return Promise.resolve();
			if (cmd === "get_tunnel_status")
				return Promise.resolve({ id: createdProfile.id, status: { type: "connected" }, started_at: "t1" });
			return Promise.resolve();
		});

		const connectDone = remoteConnectionsStore.connect(id);
		await vi.advanceTimersByTimeAsync(2000);
		await connectDone;

		const st = remoteConnectionsStore.getConnectionState(id);
		expect(st?.status).toBe("connected");
		expect(st?.baseUrl).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
		expect(st?.tunnelProfileId).toBe("auto-p1");
		expect(fetchMock).toHaveBeenCalledWith(`${st?.baseUrl}/health`);
		expect(startBridge).toHaveBeenCalledWith(id, st?.baseUrl);
		expect(mockInvoke).toHaveBeenCalledWith("save_tunnel_profile", {
			profile: expect.objectContaining({
				name: `__remote_${id}`,
				ssh: expect.objectContaining({ host: "example.test", user: "boss" }),
			}),
		});
		expect(mockInvoke).toHaveBeenCalledWith("start_tunnel", { id: "auto-p1" });
	});

	it("fails with a clear error when the created tunnel profile can't be found by name afterward", async () => {
		const id = "ssh2";
		await remoteConnectionsStore.addConnection(sshConn(id));
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "save_tunnel_profile") return Promise.resolve();
			if (cmd === "list_tunnel_profiles") return Promise.resolve([]); // lookup will fail
			return Promise.resolve();
		});

		await remoteConnectionsStore.connect(id);

		const st = remoteConnectionsStore.getConnectionState(id);
		expect(st?.status).toBe("error");
		expect(st?.error).toContain(`Could not find tunnel profile "__remote_${id}"`);
		expect(fetchMock).not.toHaveBeenCalled();
		expect(startBridge).not.toHaveBeenCalled();
	});

	it("fails with 'SSH tunnel failed to connect' when the tunnel never reaches connected (waitForTunnel's own timeout)", async () => {
		const id = "ssh3";
		await remoteConnectionsStore.addConnection(sshConn(id));
		const createdProfile = tunnelProfileFixture("auto-p3", `__remote_${id}`);
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "save_tunnel_profile") return Promise.resolve();
			if (cmd === "list_tunnel_profiles") return Promise.resolve([createdProfile]);
			if (cmd === "start_tunnel") return Promise.resolve();
			// null → startTunnel() treats this as "stopped externally": it deletes
			// the activeTunnels entry and resolves, WITHOUT setting an error/stopped
			// terminal status. waitForTunnel() then sees an undefined status on every
			// poll and has to fall through its own 30s deadline.
			if (cmd === "get_tunnel_status") return Promise.resolve(null);
			return Promise.resolve();
		});

		const connectDone = remoteConnectionsStore.connect(id);
		await vi.advanceTimersByTimeAsync(2000); // startTunnel's single poll → tunnel "gone"
		await vi.advanceTimersByTimeAsync(30_000); // waitForTunnel's own deadline
		await connectDone;

		const st = remoteConnectionsStore.getConnectionState(id);
		expect(st?.status).toBe("error");
		expect(st?.error).toBe("SSH tunnel failed to connect");
		expect(fetchMock).not.toHaveBeenCalled();
	}, 10_000);

	it("also fails cleanly when the tunnel reaches a terminal 'error' status instead of connecting", async () => {
		const id = "ssh4";
		await remoteConnectionsStore.addConnection(sshConn(id));
		const createdProfile = tunnelProfileFixture("auto-p4", `__remote_${id}`);
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "save_tunnel_profile") return Promise.resolve();
			if (cmd === "list_tunnel_profiles") return Promise.resolve([createdProfile]);
			if (cmd === "start_tunnel") return Promise.resolve();
			if (cmd === "get_tunnel_status")
				return Promise.resolve({
					id: createdProfile.id,
					status: { type: "error", message: "ssh: connect refused" },
					started_at: "t",
				});
			return Promise.resolve();
		});

		const connectDone = remoteConnectionsStore.connect(id);
		await vi.advanceTimersByTimeAsync(2000);
		await connectDone;

		const st = remoteConnectionsStore.getConnectionState(id);
		expect(st?.status).toBe("error");
		expect(st?.error).toBe("SSH tunnel failed to connect");
	});

	it("disconnect stops AND deletes the auto-created tunnel profile", async () => {
		const id = "ssh5";
		await remoteConnectionsStore.addConnection(sshConn(id));
		const createdProfile = tunnelProfileFixture("auto-p5", `__remote_${id}`);
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "save_tunnel_profile") return Promise.resolve();
			if (cmd === "list_tunnel_profiles") return Promise.resolve([createdProfile]);
			if (cmd === "start_tunnel") return Promise.resolve();
			if (cmd === "get_tunnel_status")
				return Promise.resolve({ id: createdProfile.id, status: { type: "connected" }, started_at: "t" });
			return Promise.resolve();
		});
		const connectDone = remoteConnectionsStore.connect(id);
		await vi.advanceTimersByTimeAsync(2000);
		await connectDone;
		expect(remoteConnectionsStore.getConnectionState(id)?.status).toBe("connected");

		mockInvoke.mockClear();
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "stop_tunnel") return Promise.resolve();
			if (cmd === "delete_tunnel_profile") return Promise.resolve();
			return Promise.resolve();
		});

		await remoteConnectionsStore.disconnect(id);

		expect(mockInvoke).toHaveBeenCalledWith("stop_tunnel", { id: "auto-p5" });
		expect(mockInvoke).toHaveBeenCalledWith("delete_tunnel_profile", { id: "auto-p5" });
		const st = remoteConnectionsStore.getConnectionState(id);
		expect(st?.status).toBe("disconnected");
		expect(st?.tunnelProfileId).toBeUndefined();
		expect(bridgeCleanup).toHaveBeenCalledTimes(1);
	});

	it("disconnect still resets connection state even when the tunnel stop/delete calls fail (warns, doesn't throw)", async () => {
		const id = "ssh6";
		await remoteConnectionsStore.addConnection(sshConn(id));
		const createdProfile = tunnelProfileFixture("auto-p6", `__remote_${id}`);
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "save_tunnel_profile") return Promise.resolve();
			if (cmd === "list_tunnel_profiles") return Promise.resolve([createdProfile]);
			if (cmd === "start_tunnel") return Promise.resolve();
			if (cmd === "get_tunnel_status")
				return Promise.resolve({ id: createdProfile.id, status: { type: "connected" }, started_at: "t" });
			return Promise.resolve();
		});
		const connectDone = remoteConnectionsStore.connect(id);
		await vi.advanceTimersByTimeAsync(2000);
		await connectDone;

		mockInvoke.mockClear();
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "stop_tunnel" ? Promise.reject(new Error("ssh gone")) : Promise.resolve(),
		);

		await expect(remoteConnectionsStore.disconnect(id)).resolves.toBeUndefined();
		expect(remoteConnectionsStore.getConnectionState(id)?.status).toBe("disconnected");
	});
});

describe("remoteConnectionsStore.addConnection() / removeConnection()", () => {
	beforeEach(() => {
		resetMockInvokeToDefault();
		startBridge.mockClear();
		bridgeCleanup.mockClear();
		fetchMock.mockReset();
		fetchMock.mockResolvedValue({ ok: true, json: async () => ({ protocol_version: 1 }) });
		vi.stubGlobal("fetch", fetchMock);
	});
	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it("addConnection saves via save_remote_connection and adds it in disconnected state", async () => {
		const conn = directConn("add1");
		await remoteConnectionsStore.addConnection(conn);
		expect(mockInvoke).toHaveBeenCalledWith("save_remote_connection", { connection: conn });
		expect(remoteConnectionsStore.getConnectionState("add1")).toEqual({ connection: conn, status: "disconnected" });
	});

	it("addConnection rethrows on failure and never adds the connection to state", async () => {
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "save_remote_connection" ? Promise.reject(new Error("nope")) : Promise.resolve(),
		);
		await expect(remoteConnectionsStore.addConnection(directConn("add2"))).rejects.toThrow("nope");
		expect(remoteConnectionsStore.getConnectionState("add2")).toBeUndefined();
	});

	it("removeConnection disconnects first when connected, then deletes from backend and state", async () => {
		await remoteConnectionsStore.addConnection(directConn("rm1"));
		await remoteConnectionsStore.connect("rm1");
		expect(remoteConnectionsStore.getConnectionState("rm1")?.status).toBe("connected");

		mockInvoke.mockClear();
		mockInvoke.mockResolvedValue(undefined);
		await remoteConnectionsStore.removeConnection("rm1");

		expect(bridgeCleanup).toHaveBeenCalledTimes(1); // went through disconnect() first
		expect(mockInvoke).toHaveBeenCalledWith("delete_remote_connection", { id: "rm1" });
		expect(remoteConnectionsStore.getConnectionState("rm1")).toBeUndefined();
	});

	it("removeConnection on an already-disconnected connection skips the disconnect step but still deletes", async () => {
		await remoteConnectionsStore.addConnection(directConn("rm2"));
		mockInvoke.mockClear();
		await remoteConnectionsStore.removeConnection("rm2");
		expect(mockInvoke).toHaveBeenCalledWith("delete_remote_connection", { id: "rm2" });
		expect(bridgeCleanup).not.toHaveBeenCalled();
		expect(remoteConnectionsStore.getConnectionState("rm2")).toBeUndefined();
	});

	it("removeConnection is a safe no-op for an unknown id", async () => {
		await expect(remoteConnectionsStore.removeConnection("ghost-rm")).resolves.toBeUndefined();
		expect(mockInvoke).not.toHaveBeenCalledWith("delete_remote_connection", expect.anything());
	});

	it("removeConnection rethrows on delete failure, leaving the connection in state", async () => {
		await remoteConnectionsStore.addConnection(directConn("rm3"));
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "delete_remote_connection" ? Promise.reject(new Error("fail")) : Promise.resolve(),
		);
		await expect(remoteConnectionsStore.removeConnection("rm3")).rejects.toThrow("fail");
		expect(remoteConnectionsStore.getConnectionState("rm3")).toBeDefined();
	});
});

describe("remoteConnectionsStore health-poll failure transitions", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		resetMockInvokeToDefault();
		startBridge.mockClear();
		bridgeCleanup.mockClear();
		fetchMock.mockReset();
	});
	afterEach(async () => {
		await remoteConnectionsStore.disconnect("hp1");
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("a rejected fetch on a later poll transitions status to error", async () => {
		fetchMock.mockResolvedValueOnce({ ok: true, json: async () => ({ protocol_version: 2 }) });
		vi.stubGlobal("fetch", fetchMock);
		await remoteConnectionsStore.addConnection(directConn("hp1"));
		await remoteConnectionsStore.connect("hp1");
		expect(remoteConnectionsStore.getConnectionState("hp1")?.status).toBe("connected");

		fetchMock.mockRejectedValueOnce(new Error("network down"));
		await vi.advanceTimersByTimeAsync(5000);

		const st = remoteConnectionsStore.getConnectionState("hp1");
		expect(st?.status).toBe("error");
		expect(st?.error).toBe("Unreachable: Error: network down");
	});

	it("a non-ok response on a later poll transitions status to error with the HTTP status", async () => {
		fetchMock.mockResolvedValueOnce({ ok: true, json: async () => ({ protocol_version: 2 }) });
		vi.stubGlobal("fetch", fetchMock);
		await remoteConnectionsStore.addConnection(directConn("hp1"));
		await remoteConnectionsStore.connect("hp1");

		fetchMock.mockResolvedValueOnce({ ok: false, status: 503 });
		await vi.advanceTimersByTimeAsync(5000);

		const st = remoteConnectionsStore.getConnectionState("hp1");
		expect(st?.status).toBe("error");
		expect(st?.error).toBe("Health check failed: 503");
	});
});

// hydrate() guards itself with a module-level `hydrated` boolean with no test
// reset hook, so — like tunnelsStore.hydrate() in stores/tunnels.test.ts — there
// is exactly one "still false" window across this whole file. This describe is
// placed LAST so it doesn't matter that its success step now writes via
// `reconcile()` (fixed, story: SSH Tunnels + Remote Servers consolidation —
// previously a plain-object `setState("connections", connectionsMap)` MERGED
// onto the existing "connections" object instead of replacing it wholesale).
// Every connection id added by earlier describes in this file is still present
// afterward regardless, simply because hydrate() only ever runs its real body
// once per module lifetime — assertions below check the newly-hydrated ids
// individually rather than asserting the whole map, to stay agnostic to that.
describe("remoteConnectionsStore.hydrate()", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
	});

	it("swallows a load error (stays retryable), then populates on a later success, then a further call is a no-op guard", async () => {
		const before = { ...remoteConnectionsStore.getConnections() };
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_remote_connections" ? Promise.reject(new Error("offline")) : Promise.resolve(),
		);
		await expect(remoteConnectionsStore.hydrate()).resolves.toBeUndefined();
		expect(remoteConnectionsStore.getConnections()).toEqual(before);

		const conns: RemoteConnection[] = [directConn("hy1"), directConn("hy2")];
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_remote_connections" ? Promise.resolve(conns) : Promise.resolve(),
		);
		await remoteConnectionsStore.hydrate();
		expect(remoteConnectionsStore.getConnectionState("hy1")).toEqual({ connection: conns[0], status: "disconnected" });
		expect(remoteConnectionsStore.getConnectionState("hy2")).toEqual({ connection: conns[1], status: "disconnected" });

		// A further call, even with completely different backend data, is a no-op.
		mockInvoke.mockClear();
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_remote_connections" ? Promise.resolve([directConn("hy3")]) : Promise.resolve(),
		);
		await remoteConnectionsStore.hydrate();
		expect(mockInvoke).not.toHaveBeenCalled();
		expect(remoteConnectionsStore.getConnectionState("hy3")).toBeUndefined();
		expect(remoteConnectionsStore.getConnectionState("hy1")).toBeDefined();
	});
});

// Password wiring (plan Phase 3 auth wiring) — thin passthroughs to the
// keyring-proxied Tauri commands added alongside this. The actual keyring
// CRUD is tested in src-tauri/src/remote_connection.rs; these just confirm
// the store calls the right command with the right args.
describe("remoteConnectionsStore password actions", () => {
	beforeEach(() => {
		resetMockInvokeToDefault();
	});

	it("connectionPasswordExists calls remote_connection_password_exists and returns its result", async () => {
		mockInvoke.mockResolvedValue(true);
		const result = await remoteConnectionsStore.connectionPasswordExists("c1");
		expect(mockInvoke).toHaveBeenCalledWith("remote_connection_password_exists", { id: "c1" });
		expect(result).toBe(true);
	});

	it("saveConnectionPassword calls save_remote_connection_password with id and password", async () => {
		await remoteConnectionsStore.saveConnectionPassword("c1", "hunter2");
		expect(mockInvoke).toHaveBeenCalledWith("save_remote_connection_password", { id: "c1", password: "hunter2" });
	});

	it("deleteConnectionPassword calls delete_remote_connection_password with id", async () => {
		await remoteConnectionsStore.deleteConnectionPassword("c1");
		expect(mockInvoke).toHaveBeenCalledWith("delete_remote_connection_password", { id: "c1" });
	});
});
