import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import { RemoteMachinesPanel } from "../../../components/SettingsPanel/tabs/services/RemoteMachinesPanel";
import { remoteConnectionsStore } from "../../../stores/remoteConnections";
import { mockInvoke } from "../../mocks/tauri";

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
		const { getByText } = render(() => <RemoteMachinesPanel />);
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
		});

		const { getByText } = render(() => <RemoteMachinesPanel />);
		await flushMicrotasks();

		expect(getByText("dev-box")).toBeTruthy();
		expect(getByText("http://host:9876")).toBeTruthy();
		expect(getByText("DIRECT")).toBeTruthy();
	});

	it("adding an SSH connection calls addConnection (save_remote_connection) with the built transport", async () => {
		const { getByText, getByTitle, getByPlaceholderText } = render(() => <RemoteMachinesPanel />);
		await flushMicrotasks();

		fireEvent.click(getByTitle("Add remote machine"));
		fireEvent.input(getByPlaceholderText("Name (e.g. dev-server, staging)"), { target: { value: "staging" } });
		fireEvent.input(getByPlaceholderText("Host (e.g. 192.168.1.100)"), { target: { value: "10.0.0.5" } });
		fireEvent.input(getByPlaceholderText("SSH user"), { target: { value: "deploy" } });
		fireEvent.click(getByText("Save"));
		await flushMicrotasks();

		const call = mockInvoke.mock.calls.find((c) => c[0] === "save_remote_connection");
		expect(call).toBeTruthy();
		const saved = call?.[1]?.connection;
		expect(saved).toMatchObject({
			name: "staging",
			transport: {
				type: "Ssh",
				ssh: {
					host: "10.0.0.5",
					port: 22,
					user: "deploy",
					identity_file: null,
				},
				// Fixed (plan Phase 1) — 9877 matches `RemoteConnection::new_ssh`'s
				// Rust default and what a real `tuic-remote` daemon actually
				// listens on out of the box.
				remote_daemon_port: 9877,
			},
		});
	});

	it("refuses to save a new connection with no name", async () => {
		const { getByText, getByTitle } = render(() => <RemoteMachinesPanel />);
		await flushMicrotasks();

		fireEvent.click(getByTitle("Add remote machine"));
		fireEvent.click(getByText("Save"));
		await flushMicrotasks();

		expect(mockInvoke).not.toHaveBeenCalledWith("save_remote_connection", expect.anything());
		expect(getByText("Name is required")).toBeTruthy();
	});

	it("editing an existing connection's URL calls addConnection with the updated transport", async () => {
		// Note: the inline edit panel renders only <TransportFields/> — there is
		// no "Name" input in edit mode (only the Add form has one, outside
		// TransportFields), so an existing connection's name cannot currently be
		// changed through this UI. `saveEdit()` always re-sends whatever name
		// `startEdit()` captured, unchanged — asserted below rather than assumed.
		await remoteConnectionsStore.addConnection({
			id: "edit1",
			name: "old-name",
			transport: { type: "Direct", url: "http://host:9876" },
			auth_username: "",
			enabled: true,
		});
		mockInvoke.mockClear(); // drop the seed addConnection's own save_remote_connection call
		const { getByTitle, getByText, getByDisplayValue } = render(() => <RemoteMachinesPanel />);
		await flushMicrotasks();

		fireEvent.click(getByTitle("Edit"));
		fireEvent.input(getByDisplayValue("http://host:9876"), { target: { value: "http://host2:9877" } });
		fireEvent.click(getByText("Save"));
		await flushMicrotasks();

		const call = mockInvoke.mock.calls.find((c) => c[0] === "save_remote_connection");
		expect(call?.[1]?.connection).toMatchObject({
			id: "edit1",
			name: "old-name", // preserved — the field to change it doesn't exist in edit mode
			transport: { type: "Direct", url: "http://host2:9877" },
		});
	});

	it("deleting a connection confirms first, then calls removeConnection (delete_remote_connection)", async () => {
		await remoteConnectionsStore.addConnection({
			id: "del1",
			name: "dev-box",
			transport: { type: "Direct", url: "http://host:9876" },
			auth_username: "",
			enabled: true,
		});
		vi.stubGlobal(
			"confirm",
			vi.fn(() => true),
		);
		const { getByTitle } = render(() => <RemoteMachinesPanel />);
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
		});
		vi.stubGlobal(
			"confirm",
			vi.fn(() => false),
		);
		const { getByTitle } = render(() => <RemoteMachinesPanel />);
		await flushMicrotasks();

		fireEvent.click(getByTitle("Remove"));
		await flushMicrotasks();

		expect(mockInvoke).not.toHaveBeenCalledWith("delete_remote_connection", expect.anything());
		expect(remoteConnectionsStore.getConnectionState("del2")).toBeDefined();
	});

	it("Connect button on a disconnected row connects it (health-checks, shows Connected)", async () => {
		await remoteConnectionsStore.addConnection({
			id: "conn1",
			name: "dev-box",
			transport: { type: "Direct", url: "http://host:9876" },
			auth_username: "",
			enabled: true,
		});
		const { getByText } = render(() => <RemoteMachinesPanel />);
		await flushMicrotasks();

		fireEvent.click(getByText("Connect"));
		await flushMicrotasks();

		expect(remoteConnectionsStore.getConnectionState("conn1")?.status).toBe("connected");
		await remoteConnectionsStore.disconnect("conn1"); // stop the health-poll interval before the file ends
	});

	it("Disconnect button on a connected row disconnects it", async () => {
		await remoteConnectionsStore.addConnection({
			id: "conn2",
			name: "dev-box-2",
			transport: { type: "Direct", url: "http://host2:9876" },
			auth_username: "",
			enabled: true,
		});
		await remoteConnectionsStore.connect("conn2");
		expect(remoteConnectionsStore.getConnectionState("conn2")?.status).toBe("connected");

		const { getByText } = render(() => <RemoteMachinesPanel />);
		await flushMicrotasks();

		fireEvent.click(getByText("Disconnect"));
		await flushMicrotasks();

		expect(remoteConnectionsStore.getConnectionState("conn2")?.status).toBe("disconnected");
	});
});
