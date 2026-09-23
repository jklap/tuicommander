import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Mock the IPC boundary only — the poll loop, state transitions, and interval
// teardown are the real logic under test.
vi.mock("../../invoke", () => ({ invoke: vi.fn() }));

import { invoke } from "../../invoke";
import type { ActiveTunnel, TunnelProfile } from "../../stores/tunnels";
import { tunnelsStore } from "../../stores/tunnels";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

/** Full-shaped fixture — every action reads/writes the whole TunnelProfile object. */
function makeProfile(id: string, overrides: Partial<TunnelProfile> = {}): TunnelProfile {
	return {
		id,
		name: `profile-${id}`,
		ssh: {
			host: "example.test",
			port: 22,
			user: "boss",
			identity_file: null,
			server_alive_interval: 15,
			server_alive_count_max: 3,
			strict_host_key_checking: "AcceptNew",
		},
		forwards: [],
		auto_connect: false,
		...overrides,
	};
}

/** Route invoke by command: start_tunnel always ok; get_tunnel_status drains a queue. */
function withStatusSequence(statuses: Array<unknown>) {
	const queue = [...statuses];
	mockInvoke.mockImplementation((cmd: string) => {
		if (cmd === "start_tunnel") return Promise.resolve();
		if (cmd === "get_tunnel_status") {
			const next = queue.length ? queue.shift() : undefined;
			return next instanceof Error ? Promise.reject(next) : Promise.resolve(next);
		}
		return Promise.resolve();
	});
}

describe("tunnelsStore.startTunnel poll loop", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		vi.useFakeTimers();
	});
	afterEach(() => {
		vi.useRealTimers();
	});

	it("polls every 2s until a terminal state (connected), then stops", async () => {
		withStatusSequence([
			{ id: "t1", status: { type: "starting" }, started_at: "x" }, // non-terminal → keep polling
			{ id: "t1", status: { type: "connected" }, started_at: "x" }, // terminal → resolve
		]);

		const done = tunnelsStore.startTunnel("t1");
		await vi.advanceTimersByTimeAsync(0); // flush start_tunnel + optimistic "starting"
		expect(tunnelsStore.getTunnelStatus("t1")).toEqual({ type: "starting" });

		await vi.advanceTimersByTimeAsync(2000); // poll #1 → still starting
		await vi.advanceTimersByTimeAsync(2000); // poll #2 → connected, clearInterval
		await done;

		expect(tunnelsStore.getTunnelStatus("t1")).toEqual({ type: "connected" });
		// Exactly two status polls happened; the interval was cleared afterwards.
		const polls = mockInvoke.mock.calls.filter((c) => c[0] === "get_tunnel_status").length;
		expect(polls).toBe(2);
		await vi.advanceTimersByTimeAsync(4000); // no further polls after terminal
		expect(mockInvoke.mock.calls.filter((c) => c[0] === "get_tunnel_status").length).toBe(2);
	});

	it("deletes the tunnel and resolves when status returns null (stopped externally)", async () => {
		withStatusSequence([null]);
		const done = tunnelsStore.startTunnel("t2");
		await vi.advanceTimersByTimeAsync(0);
		expect(tunnelsStore.getTunnelStatus("t2")).toEqual({ type: "starting" });
		await vi.advanceTimersByTimeAsync(2000);
		await done;
		expect(tunnelsStore.getTunnelStatus("t2")).toBeUndefined();
	});

	it("marks the tunnel errored and resolves when a poll throws", async () => {
		withStatusSequence([new Error("boom")]);
		const done = tunnelsStore.startTunnel("t3");
		await vi.advanceTimersByTimeAsync(0);
		await vi.advanceTimersByTimeAsync(2000);
		await done;
		expect(tunnelsStore.getTunnelStatus("t3")).toEqual({ type: "error", message: "Error: boom" });
	});

	it("rethrows and never enters the poll loop when start_tunnel fails", async () => {
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "start_tunnel" ? Promise.reject(new Error("nope")) : Promise.resolve(),
		);
		await expect(tunnelsStore.startTunnel("t4")).rejects.toThrow("nope");
		expect(tunnelsStore.getTunnelStatus("t4")).toBeUndefined(); // no optimistic entry
		expect(mockInvoke.mock.calls.some((c) => c[0] === "get_tunnel_status")).toBe(false);
	});
});

// `hydrate()` guards itself with a module-level `hydrated` boolean that never resets
// within this file's single module instance, so there is exactly one "still false"
// window across the whole file. All three hydrate semantics (error stays retryable,
// success populates + auto-connects, a further call is a no-op) are exercised as one
// ordered sequence within a single test rather than split across `it()`s that would
// each need a truly fresh module (see stores/remoteConnections.test.ts for the same
// constraint, documented in more detail there).
describe("tunnelsStore.hydrate()", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		vi.useFakeTimers();
	});
	afterEach(() => {
		vi.useRealTimers();
	});

	it("swallows a load error (stays retryable), then a later call populates + auto-connects, then a further call is a no-op guard", async () => {
		// 1) Backend unreachable — hydrate() must not throw, and must leave the
		//    guard false so a later call can actually retry.
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_tunnel_profiles" || cmd === "list_active_tunnels"
				? Promise.reject(new Error("offline"))
				: Promise.resolve(),
		);
		const activeBefore = { ...tunnelsStore.getActiveTunnels() };
		await expect(tunnelsStore.hydrate()).resolves.toBeUndefined();
		expect(tunnelsStore.getProfiles()).toEqual([]);
		// setState("activeTunnels", ...) merges rather than replacing (see the
		// dedicated merge-semantics test below), so "the failed call touched
		// nothing" is asserted as "unchanged from before", not "now empty".
		expect(tunnelsStore.getActiveTunnels()).toEqual(activeBefore);

		// 2) Backend responds: p1 (auto_connect, not active) should get auto-started;
		//    p2 (auto_connect=false) and p3 (auto_connect but already active) should not.
		const profiles = [
			makeProfile("p1", { auto_connect: true }),
			makeProfile("p2"),
			makeProfile("p3", { auto_connect: true }),
		];
		const active: ActiveTunnel[] = [{ id: "p3", status: { type: "connected" }, started_at: "t0" }];
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_tunnel_profiles") return Promise.resolve(profiles);
			if (cmd === "list_active_tunnels") return Promise.resolve(active);
			if (cmd === "start_tunnel") return Promise.resolve();
			if (cmd === "get_tunnel_status")
				return Promise.resolve({ id: "p1", status: { type: "connected" }, started_at: "t1" });
			return Promise.resolve();
		});
		await tunnelsStore.hydrate();
		// The auto-connect's startTunnel() is fire-and-forget from hydrate()'s
		// perspective — flush its poll loop so nothing leaks past this test.
		await vi.advanceTimersByTimeAsync(2000);

		expect(tunnelsStore.getProfiles().map((p) => p.id)).toEqual(["p1", "p2", "p3"]);
		expect(mockInvoke).toHaveBeenCalledWith("start_tunnel", { id: "p1" });
		expect(mockInvoke).not.toHaveBeenCalledWith("start_tunnel", { id: "p2" });
		expect(mockInvoke).not.toHaveBeenCalledWith("start_tunnel", { id: "p3" });
		expect(tunnelsStore.getTunnelStatus("p1")).toEqual({ type: "connected" });
		expect(tunnelsStore.getTunnelStatus("p3")).toEqual({ type: "connected" }); // from list_active_tunnels

		// 3) A further call, even with completely different backend data, is a no-op.
		mockInvoke.mockClear();
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_tunnel_profiles") return Promise.resolve([makeProfile("zzz")]);
			if (cmd === "list_active_tunnels") return Promise.resolve([]);
			return Promise.resolve();
		});
		await tunnelsStore.hydrate();
		expect(mockInvoke).not.toHaveBeenCalled();
		expect(tunnelsStore.getProfiles().map((p) => p.id)).toEqual(["p1", "p2", "p3"]);
	});
});

describe("tunnelsStore.refreshProfiles() / refreshActiveTunnels()", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
	});

	it("refreshProfiles replaces state.profiles wholesale from list_tunnel_profiles", async () => {
		const profiles = [makeProfile("a"), makeProfile("b")];
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_tunnel_profiles" ? Promise.resolve(profiles) : Promise.resolve(),
		);
		await tunnelsStore.refreshProfiles();
		expect(tunnelsStore.getProfiles()).toEqual(profiles);

		// A second call with fewer profiles fully replaces, not merges.
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_tunnel_profiles" ? Promise.resolve([makeProfile("a")]) : Promise.resolve(),
		);
		await tunnelsStore.refreshProfiles();
		expect(tunnelsStore.getProfiles()).toEqual([makeProfile("a")]);
	});

	it("refreshProfiles swallows a backend error without throwing, leaving state as-is", async () => {
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_tunnel_profiles" ? Promise.resolve([makeProfile("keep-me")]) : Promise.resolve(),
		);
		await tunnelsStore.refreshProfiles();

		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_tunnel_profiles" ? Promise.reject(new Error("down")) : Promise.resolve(),
		);
		await expect(tunnelsStore.refreshProfiles()).resolves.toBeUndefined();
		expect(tunnelsStore.getProfiles()).toEqual([makeProfile("keep-me")]);
	});

	it("refreshActiveTunnels writes the ids the backend returned, keyed by id", async () => {
		const active: ActiveTunnel[] = [
			{ id: "t1", status: { type: "connected" }, started_at: "x" },
			{ id: "t2", status: { type: "error", message: "boom" }, started_at: "y" },
		];
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_active_tunnels" ? Promise.resolve(active) : Promise.resolve(),
		);
		await tunnelsStore.refreshActiveTunnels();
		expect(tunnelsStore.getActiveTunnels()).toEqual(expect.objectContaining({ t1: active[0], t2: active[1] }));
	});

	// `setState("activeTunnels", activeTunnelsMap)` passes a plain object, which
	// SolidJS's store setter MERGES onto the existing "activeTunnels" object key
	// by key — it does not delete keys the plain-object form omits (only `produce`
	// or an explicit function form does that). Concretely: a tunnel that existed
	// in a previous refresh but is no longer in the backend's `list_active_tunnels`
	// response is never removed by refreshActiveTunnels() — it lingers until
	// something else (stopTunnel's `produce` delete, or the poll loop's own
	// "tunnel gone" branch) removes that specific key. This is current behavior,
	// not a fix — documented here so a future change to "replace wholesale"
	// semantics doesn't silently break without a failing test calling it out.
	it("does NOT remove a stale id that the backend stopped reporting (merge, not replace — current behavior)", async () => {
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_active_tunnels"
				? Promise.resolve([{ id: "stale1", status: { type: "connected" }, started_at: "x" }])
				: Promise.resolve(),
		);
		await tunnelsStore.refreshActiveTunnels();
		expect(tunnelsStore.getTunnelStatus("stale1")).toEqual({ type: "connected" });

		// Backend now reports a totally different set, with "stale1" gone.
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_active_tunnels"
				? Promise.resolve([{ id: "other", status: { type: "connected" }, started_at: "y" }])
				: Promise.resolve(),
		);
		await tunnelsStore.refreshActiveTunnels();

		expect(tunnelsStore.getTunnelStatus("other")).toEqual({ type: "connected" });
		expect(tunnelsStore.getTunnelStatus("stale1")).toEqual({ type: "connected" }); // lingers — not removed
	});

	it("refreshActiveTunnels swallows a backend error without throwing, leaving state as-is", async () => {
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_active_tunnels"
				? Promise.resolve([{ id: "keep", status: { type: "connected" }, started_at: "x" }])
				: Promise.resolve(),
		);
		await tunnelsStore.refreshActiveTunnels();

		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_active_tunnels" ? Promise.reject(new Error("down")) : Promise.resolve(),
		);
		await expect(tunnelsStore.refreshActiveTunnels()).resolves.toBeUndefined();
		expect(tunnelsStore.getTunnelStatus("keep")).toEqual({ type: "connected" });
	});
});

describe("tunnelsStore profile CRUD (createProfile / updateProfile / deleteProfile)", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
	});

	it("createProfile saves via save_tunnel_profile (no id) then refreshes the profile list", async () => {
		const { id: _omit, ...withoutId } = makeProfile("new1");
		const created = makeProfile("new1");
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "save_tunnel_profile") return Promise.resolve();
			if (cmd === "list_tunnel_profiles") return Promise.resolve([created]);
			return Promise.resolve();
		});

		await tunnelsStore.createProfile(withoutId);

		expect(mockInvoke).toHaveBeenCalledWith("save_tunnel_profile", { profile: withoutId });
		expect(mockInvoke).toHaveBeenCalledWith("list_tunnel_profiles");
		expect(tunnelsStore.getProfiles()).toEqual([created]);
	});

	it("createProfile rethrows and skips the refresh when save_tunnel_profile fails", async () => {
		const { id: _omit, ...withoutId } = makeProfile("new2");
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "save_tunnel_profile" ? Promise.reject(new Error("nope")) : Promise.resolve(),
		);
		await expect(tunnelsStore.createProfile(withoutId)).rejects.toThrow("nope");
		expect(mockInvoke).not.toHaveBeenCalledWith("list_tunnel_profiles");
	});

	it("updateProfile saves the full profile (with id) then refreshes", async () => {
		const updated = makeProfile("existing1", { name: "renamed" });
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "save_tunnel_profile") return Promise.resolve();
			if (cmd === "list_tunnel_profiles") return Promise.resolve([updated]);
			return Promise.resolve();
		});

		await tunnelsStore.updateProfile(updated);

		expect(mockInvoke).toHaveBeenCalledWith("save_tunnel_profile", { profile: updated });
		expect(tunnelsStore.getProfiles()).toEqual([updated]);
	});

	it("updateProfile rethrows and skips the refresh when save_tunnel_profile fails", async () => {
		const updated = makeProfile("existing2");
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "save_tunnel_profile" ? Promise.reject(new Error("boom")) : Promise.resolve(),
		);
		await expect(tunnelsStore.updateProfile(updated)).rejects.toThrow("boom");
		expect(mockInvoke).not.toHaveBeenCalledWith("list_tunnel_profiles");
	});

	it("deleteProfile deletes by id then refreshes", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "delete_tunnel_profile") return Promise.resolve();
			if (cmd === "list_tunnel_profiles") return Promise.resolve([]);
			return Promise.resolve();
		});

		await tunnelsStore.deleteProfile("existing1");

		expect(mockInvoke).toHaveBeenCalledWith("delete_tunnel_profile", { id: "existing1" });
		expect(mockInvoke).toHaveBeenCalledWith("list_tunnel_profiles");
		expect(tunnelsStore.getProfiles()).toEqual([]);
	});

	it("deleteProfile rethrows and skips the refresh when delete_tunnel_profile fails", async () => {
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "delete_tunnel_profile" ? Promise.reject(new Error("boom")) : Promise.resolve(),
		);
		await expect(tunnelsStore.deleteProfile("x")).rejects.toThrow("boom");
		expect(mockInvoke).not.toHaveBeenCalledWith("list_tunnel_profiles");
	});
});

describe("tunnelsStore.stopTunnel()", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		vi.useFakeTimers();
	});
	afterEach(() => {
		vi.useRealTimers();
	});

	/** Get a tunnel into "connected" via the real startTunnel() poll loop. */
	async function seedConnected(id: string): Promise<void> {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "start_tunnel") return Promise.resolve();
			if (cmd === "get_tunnel_status") return Promise.resolve({ id, status: { type: "connected" }, started_at: "x" });
			return Promise.resolve();
		});
		const done = tunnelsStore.startTunnel(id);
		await vi.advanceTimersByTimeAsync(2000);
		await done;
	}

	it("stops the tunnel via stop_tunnel and removes it from activeTunnels on success", async () => {
		await seedConnected("s1");
		mockInvoke.mockImplementation((cmd: string) => (cmd === "stop_tunnel" ? Promise.resolve() : Promise.resolve()));

		await tunnelsStore.stopTunnel("s1");

		expect(mockInvoke).toHaveBeenCalledWith("stop_tunnel", { id: "s1" });
		expect(tunnelsStore.getTunnelStatus("s1")).toBeUndefined();
	});

	it("rethrows and keeps the entry in activeTunnels when stop_tunnel fails", async () => {
		await seedConnected("s2");
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "stop_tunnel" ? Promise.reject(new Error("fail")) : Promise.resolve(),
		);

		await expect(tunnelsStore.stopTunnel("s2")).rejects.toThrow("fail");
		expect(tunnelsStore.getTunnelStatus("s2")).toEqual({ type: "connected" });
	});
});
