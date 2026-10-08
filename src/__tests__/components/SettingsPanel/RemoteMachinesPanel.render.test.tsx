import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

// main's store follows the backend's `remote-connection-status` pushes; capture
// the handler it subscribes with so a test can play the backend's role.
const statusHandlers = vi.hoisted(() => ({ current: undefined as ((payload: unknown) => void) | undefined }));
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	subscribeEvents: async (handlers: Record<string, (payload: unknown) => void>) => {
		statusHandlers.current = handlers["remote-connection-status"] ?? statusHandlers.current;
		return () => {};
	},
}));

import { RemoteMachinesPanel } from "../../../components/SettingsPanel/tabs/services/RemoteMachinesPanel";
import { remoteConnectionsStore } from "../../../stores/remoteConnections";
import { mockInvoke } from "../../mocks/tauri";

// Creating and editing moved to the Remote Servers page's merged editor
// (RemoteServersTab.render.test.tsx); this list only reports which row to edit.
const handlers = { onEdit: vi.fn(), onAddFromHost: vi.fn() };

async function flushMicrotasks(): Promise<void> {
	await new Promise<void>((resolve) => setImmediate(resolve));
}

describe("RemoteMachinesPanel", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
		vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: true, json: async () => ({ protocol_version: 1 }) }));
	});

	afterEach(async () => {
		cleanup();
		// remoteConnectionsStore is a real singleton shared across every test in
		// this file (addConnection has no guard, so seeded rows would otherwise
		// accumulate and make later `getByText("Connect")`-style queries
		// ambiguous). Tear down whatever this test added, disconnecting first so
		// no health-poll interval leaks either.
		mockInvoke.mockResolvedValue(undefined);
		for (const id of Object.keys(remoteConnectionsStore.getConnections())) {
			await remoteConnectionsStore.removeConnection(id).catch(() => {});
		}
		vi.unstubAllGlobals();
		vi.restoreAllMocks();
		await flushMicrotasks();
	});

	// Runs first, deliberately: remoteConnectionsStore is a real singleton shared
	// across every test in this file, and every other test below seeds it via the
	// unguarded addConnection() action. Ordering this test first is what keeps
	// "no connections yet" meaningful.
	it("renders the empty state and hydrates on mount", async () => {
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_remote_connections" ? Promise.resolve([]) : Promise.resolve(undefined),
		);
		const { getByText } = render(() => <RemoteMachinesPanel {...handlers} />);
		await flushMicrotasks();

		expect(mockInvoke).toHaveBeenCalledWith("list_remote_connections");
		expect(getByText(/No remote machines configured/)).toBeTruthy();
	});

	it("renders an existing connection's name, transport summary, and status", async () => {
		await remoteConnectionsStore.addConnection({
			id: "render1",
			name: "dev-box",
			transport: { type: "Direct", url: "http://host:9876" },
			auth_username: "",
			enabled: true,
			deploy: "never",
			survive_secs: 1800,
		});

		const { getByText } = render(() => <RemoteMachinesPanel {...handlers} />);
		await flushMicrotasks();

		expect(getByText("dev-box")).toBeTruthy();
		expect(getByText("http://host:9876")).toBeTruthy();
		expect(getByText("DIRECT")).toBeTruthy();
	});

	it("Edit hands the row's connection to the page's editor", async () => {
		await remoteConnectionsStore.addConnection({
			id: "edit1",
			name: "old-name",
			transport: { type: "Direct", url: "http://host:9876" },
			auth_username: "",
			enabled: true,
			deploy: "never",
			survive_secs: 1800,
		});
		const { getByTitle } = render(() => <RemoteMachinesPanel {...handlers} />);
		await flushMicrotasks();

		fireEvent.click(getByTitle("Edit"));

		expect(handlers.onEdit).toHaveBeenCalledWith(expect.objectContaining({ id: "edit1", name: "old-name" }));
	});

	it("clicking a discovered SSH host asks the page to add a connection prefilled from it", async () => {
		const host = { host: "build-box", target: "10.0.0.9", user: "ci", port: 2222, source: "config" as const };
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "list_discovered_ssh_hosts"
				? Promise.resolve({ hosts: [host], hashed_count: 0 })
				: Promise.resolve(undefined),
		);
		const { getByTitle } = render(() => <RemoteMachinesPanel {...handlers} />);
		await flushMicrotasks();

		fireEvent.click(getByTitle("Add a connection prefilled with this host"));

		expect(handlers.onAddFromHost).toHaveBeenCalledWith(host);
	});

	it("deleting a connection confirms first, then calls removeConnection (delete_remote_connection)", async () => {
		await remoteConnectionsStore.addConnection({
			id: "del1",
			name: "dev-box",
			transport: { type: "Direct", url: "http://host:9876" },
			auth_username: "",
			enabled: true,
			deploy: "never",
			survive_secs: 1800,
		});
		vi.stubGlobal(
			"confirm",
			vi.fn(() => true),
		);
		const { getByTitle } = render(() => <RemoteMachinesPanel {...handlers} />);
		await flushMicrotasks();

		fireEvent.click(getByTitle("Remove"));
		await flushMicrotasks();

		expect(mockInvoke).toHaveBeenCalledWith("delete_remote_connection", { id: "del1" });
		expect(remoteConnectionsStore.getConnectionState("del1")).toBeUndefined();
	});

	it("declining the confirm dialog never calls removeConnection", async () => {
		await remoteConnectionsStore.addConnection({
			id: "del2",
			name: "dev-box",
			transport: { type: "Direct", url: "http://host:9876" },
			auth_username: "",
			enabled: true,
			deploy: "never",
			survive_secs: 1800,
		});
		vi.stubGlobal(
			"confirm",
			vi.fn(() => false),
		);
		const { getByTitle } = render(() => <RemoteMachinesPanel {...handlers} />);
		await flushMicrotasks();

		fireEvent.click(getByTitle("Remove"));
		await flushMicrotasks();

		expect(mockInvoke).not.toHaveBeenCalledWith("delete_remote_connection", expect.anything());
		expect(remoteConnectionsStore.getConnectionState("del2")).toBeDefined();
	});

	// Ported to main's design: the connection state machine lives in Rust
	// (`remote_runtime.rs`), the store only asks the backend and renders the
	// `remote-connection-status` pushes (wip's version asserted a frontend
	// health check flipping the row to Connected).
	it("Connect button on a disconnected row asks the backend to connect it", async () => {
		await remoteConnectionsStore.addConnection({
			id: "conn1",
			name: "dev-box",
			transport: { type: "Direct", url: "http://host:9876" },
			auth_username: "",
			enabled: true,
			deploy: "never",
			survive_secs: 1800,
		});
		const { getByText } = render(() => <RemoteMachinesPanel {...handlers} />);
		await flushMicrotasks();

		fireEvent.click(getByText("Connect"));
		await flushMicrotasks();

		expect(mockInvoke).toHaveBeenCalledWith("connect_remote_connection", { id: "conn1" });
	});

	it("Disconnect button on a connected row asks the backend to disconnect it", async () => {
		await remoteConnectionsStore.addConnection({
			id: "conn2",
			name: "dev-box-2",
			transport: { type: "Direct", url: "http://host2:9876" },
			auth_username: "",
			enabled: true,
			deploy: "never",
			survive_secs: 1800,
		});
		// The backend's status push is the only thing that marks a row connected.
		expect(statusHandlers.current, "hydrate subscribes to remote-connection-status").toBeTruthy();
		statusHandlers.current?.({ id: "conn2", status: "connected", base_url: "http://host2:9876" });
		expect(remoteConnectionsStore.getConnectionState("conn2")?.status).toBe("connected");

		const { getByText } = render(() => <RemoteMachinesPanel {...handlers} />);
		await flushMicrotasks();

		fireEvent.click(getByText("Disconnect"));
		await flushMicrotasks();

		expect(mockInvoke).toHaveBeenCalledWith("disconnect_remote_connection", { id: "conn2" });
	});
});
