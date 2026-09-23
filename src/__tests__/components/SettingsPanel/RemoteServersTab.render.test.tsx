import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import { RemoteServersTab } from "../../../components/SettingsPanel/tabs/RemoteServersTab";
import { remoteConnectionsStore } from "../../../stores/remoteConnections";
import { tunnelsStore } from "../../../stores/tunnels";
import { mockInvoke } from "../../mocks/tauri";

async function flushMicrotasks(): Promise<void> {
	await new Promise<void>((resolve) => setImmediate(resolve));
}

/** Fields are laid out as sibling <label>/<input> pairs, same convention as
 * TunnelEditorModal.test.ts's own `getFieldInput` helper. */
function getFieldInput(container: HTMLElement, labelText: string): HTMLInputElement {
	const label = Array.from(container.querySelectorAll("label")).find((el) => el.textContent === labelText);
	if (!label?.parentElement) throw new Error(`Could not find a field group for label "${labelText}"`);
	const input = label.parentElement.querySelector("input");
	if (!input) throw new Error(`Could not find an <input> near label "${labelText}"`);
	return input as HTMLInputElement;
}

function baseInvoke(cmd: string): Promise<unknown> {
	if (cmd === "list_remote_connections") return Promise.resolve([]);
	if (cmd === "list_tunnel_profiles") return Promise.resolve([]);
	if (cmd === "list_active_tunnels") return Promise.resolve([]);
	if (cmd === "list_ssh_config_hosts") return Promise.resolve([]);
	if (cmd === "list_ssh_agent_keys") return Promise.resolve({ keys: [], agent_type: "" });
	if (cmd === "remote_connection_password_exists") return Promise.resolve(false);
	if (cmd === "list_discovered_ssh_hosts") return Promise.resolve({ hosts: [], hashed_count: 0 });
	return Promise.resolve(undefined);
}

describe("RemoteServersTab", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		mockInvoke.mockImplementation(baseInvoke);
		vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: true, json: async () => ({ protocol_version: 1 }) }));
	});

	afterEach(async () => {
		cleanup();
		mockInvoke.mockImplementation(baseInvoke);
		for (const id of Object.keys(remoteConnectionsStore.getConnections())) {
			await remoteConnectionsStore.removeConnection(id).catch(() => {});
		}
		vi.unstubAllGlobals();
		vi.restoreAllMocks();
		await flushMicrotasks();
	});

	it("renders the heading and empty states", async () => {
		const { getByText } = render(() => <RemoteServersTab />);
		await flushMicrotasks();

		expect(getByText("Remote Servers")).toBeTruthy();
		expect(getByText(/No tunnel profiles yet/)).toBeTruthy();
		expect(getByText(/No remote machines configured/)).toBeTruthy();
	});

	it("Add Connection opens the merged editor with a Name field and a 4-option Kind dropdown", async () => {
		const { getByText, container } = render(() => <RemoteServersTab />);
		await flushMicrotasks();

		fireEvent.click(getByText("Add Connection"));
		await flushMicrotasks();

		expect(getFieldInput(container, "Name").value).toBe("");
		const kindSelect = container.querySelector("select") as HTMLSelectElement;
		const options = Array.from(kindSelect.querySelectorAll("option")).map((o) => o.textContent);
		expect(options).toEqual(["SSH Tunnel", "Remote Server — SSH", "Remote Server — Direct", "Remote Server — Local"]);
	});

	it("adding a Remote Server — Direct connection saves via save_remote_connection with the built transport", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "save_remote_connection") return Promise.resolve();
			return baseInvoke(cmd);
		});
		const { getByText, container } = render(() => <RemoteServersTab />);
		await flushMicrotasks();

		fireEvent.click(getByText("Add Connection"));
		await flushMicrotasks();

		fireEvent.input(getFieldInput(container, "Name"), { target: { value: "staging" } });

		const kindSelect = container.querySelector("select") as HTMLSelectElement;
		fireEvent.change(kindSelect, { target: { value: "RemoteDirect" } });
		await flushMicrotasks();

		fireEvent.input(getFieldInput(container, "URL"), { target: { value: "http://10.0.0.5:9877" } });
		fireEvent.click(getByText("Save"));
		await flushMicrotasks();

		const call = mockInvoke.mock.calls.find((c) => c[0] === "save_remote_connection");
		expect(call).toBeTruthy();
		expect(call?.[1]?.connection).toMatchObject({
			name: "staging",
			transport: { type: "Direct", url: "http://10.0.0.5:9877" },
		});
	});

	it("editing an existing remote connection shows its Name field pre-filled (bug fix: rename without delete+recreate)", async () => {
		await remoteConnectionsStore.addConnection({
			id: "edit1",
			name: "old-name",
			transport: { type: "Direct", url: "http://host:9876" },
			auth_username: "",
			enabled: true,
			deploy: "never",
			survive_secs: 1800,
		});
		mockInvoke.mockClear();
		const { getByTitle, getByDisplayValue } = render(() => <RemoteServersTab />);
		await flushMicrotasks();

		fireEvent.click(getByTitle("Edit"));
		await flushMicrotasks();

		expect(getByDisplayValue("old-name")).toBeTruthy();
	});

	it("renders an existing tunnel profile and remote connection in their respective lists", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_tunnel_profiles")
				return Promise.resolve([
					{
						id: "t1",
						name: "prod tunnel",
						ssh: {
							host: "prod.test",
							port: 22,
							user: "boss",
							identity_file: null,
							server_alive_interval: 15,
							server_alive_count_max: 3,
							strict_host_key_checking: "Yes",
							compression: true,
						},
						forwards: [],
						auto_connect: false,
					},
				]);
			return baseInvoke(cmd);
		});
		await tunnelsStore.refreshProfiles();
		await remoteConnectionsStore.addConnection({
			id: "rc1",
			name: "dev-box",
			transport: { type: "Direct", url: "http://host:9876" },
			auth_username: "",
			enabled: true,
			deploy: "never",
			survive_secs: 1800,
		});

		const { getByText } = render(() => <RemoteServersTab />);
		await flushMicrotasks();

		expect(getByText("prod tunnel")).toBeTruthy();
		expect(getByText("dev-box")).toBeTruthy();
	});
	/** Open the editor, pick a Kind, fill Name (+ SSH host/user for SSH kinds). */
	async function openEditor(kind: string) {
		const view = render(() => <RemoteServersTab />);
		await flushMicrotasks();
		fireEvent.click(view.getByText("Add Connection"));
		await flushMicrotasks();
		fireEvent.change(view.container.querySelector("select") as HTMLSelectElement, { target: { value: kind } });
		await flushMicrotasks();
		return view;
	}

	it("a new Remote Server — SSH connection saves the runtime's defaults (9877, accept-new, compression, never deploy)", async () => {
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "save_remote_connection" ? Promise.resolve() : baseInvoke(cmd),
		);
		const { getByText, container } = await openEditor("RemoteSsh");

		fireEvent.input(getFieldInput(container, "Name"), { target: { value: "staging" } });
		fireEvent.input(getFieldInput(container, "Host"), { target: { value: "10.0.0.5" } });
		fireEvent.input(getFieldInput(container, "User"), { target: { value: "deploy" } });
		fireEvent.click(getByText("Save"));
		await flushMicrotasks();

		const saved = mockInvoke.mock.calls.find((c) => c[0] === "save_remote_connection")?.[1]?.connection;
		expect(saved).toMatchObject({
			name: "staging",
			enabled: true,
			auto_update: false,
			deploy: "never",
			survive_secs: 1800,
			transport: {
				type: "Ssh",
				ssh: { host: "10.0.0.5", port: 22, user: "deploy", identity_file: null, compression: true },
				remote_daemon_port: 9877,
			},
		});
		// No username typed: saved as `null`, never `""` (the backend sends an
		// empty one, which every daemon refuses).
		expect(saved.auth_username).toBeNull();
		// The runtime always opens a remote server's tunnel with accept-new.
		expect(saved.transport.ssh.strict_host_key_checking).toBe("AcceptNew");
	});

	it("a Remote Server never offers StrictHostKeyChecking=Yes, which its runtime would ignore", async () => {
		const { container, getByText } = await openEditor("RemoteSsh");
		const label = Array.from(container.querySelectorAll("label")).find(
			(el) => el.textContent === "StrictHostKeyChecking",
		);
		const select = label?.parentElement?.querySelector("select") as HTMLSelectElement;
		expect(select.disabled).toBe(true);
		expect(Array.from(select.options).map((o) => o.value)).toEqual(["AcceptNew"]);
		expect(getByText(/always accepts a new host's key/)).toBeTruthy();
	});

	it("an SSH Tunnel keeps both host-key choices", async () => {
		const { container } = await openEditor("SshTunnel");
		const label = Array.from(container.querySelectorAll("label")).find(
			(el) => el.textContent === "StrictHostKeyChecking",
		);
		const select = label?.parentElement?.querySelector("select") as HTMLSelectElement;
		expect(select.disabled).toBe(false);
		expect(Array.from(select.options).map((o) => o.value)).toEqual(["AcceptNew", "Yes"]);
	});

	it("refuses to save a new connection with no name", async () => {
		const { getByText } = await openEditor("RemoteDirect");
		fireEvent.click(getByText("Save"));
		await flushMicrotasks();

		expect(mockInvoke).not.toHaveBeenCalledWith("save_remote_connection", expect.anything());
		expect(getByText("Name is required")).toBeTruthy();
	});

	it("a typed password is stored after the connection exists, keyed by its id", async () => {
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "save_remote_connection" ? Promise.resolve() : baseInvoke(cmd),
		);
		const { getByText, container } = await openEditor("RemoteDirect");
		fireEvent.input(getFieldInput(container, "Name"), { target: { value: "lan" } });
		fireEvent.input(getFieldInput(container, "URL"), { target: { value: "http://10.0.0.5:9877" } });
		fireEvent.input(getFieldInput(container, "Auth password (optional)"), { target: { value: "s3cret" } });
		fireEvent.click(getByText("Save"));
		await flushMicrotasks();

		const commands = mockInvoke.mock.calls.map((c) => c[0]);
		const saveAt = commands.indexOf("save_remote_connection");
		const passwordAt = commands.indexOf("set_remote_connection_password");
		expect(saveAt).toBeGreaterThanOrEqual(0);
		expect(passwordAt).toBeGreaterThan(saveAt);
		const id = mockInvoke.mock.calls[saveAt][1]?.connection.id;
		expect(mockInvoke.mock.calls[passwordAt][1]).toEqual({ id, password: "s3cret" });
	});

	it("editing a connection keeps its id and the fields the form does not own, and saves the change", async () => {
		await remoteConnectionsStore.addConnection({
			id: "edit2",
			name: "old-name",
			transport: { type: "Direct", url: "http://host:9876" },
			auth_username: "boss",
			enabled: false,
			auto_update: true,
			deploy: "never",
			survive_secs: 3600,
		});
		mockInvoke.mockClear();
		const { getAllByTitle, getByText, getByDisplayValue } = render(() => <RemoteServersTab />);
		await flushMicrotasks();

		// The tunnel list's rows carry a text "Edit" button too; a connection
		// row's is the icon one.
		const editConnection = getAllByTitle("Edit").find((el) => !el.textContent?.trim());
		fireEvent.click(editConnection as HTMLElement);
		await flushMicrotasks();
		fireEvent.input(getByDisplayValue("old-name"), { target: { value: "new-name" } });
		fireEvent.input(getByDisplayValue("http://host:9876"), { target: { value: "http://host2:9877" } });
		fireEvent.click(getByText("Save"));
		await flushMicrotasks();

		const saved = mockInvoke.mock.calls.find((c) => c[0] === "save_remote_connection")?.[1]?.connection;
		expect(saved).toMatchObject({
			id: "edit2",
			name: "new-name",
			transport: { type: "Direct", url: "http://host2:9877" },
			auth_username: "boss",
			enabled: false,
			auto_update: true,
			survive_secs: 3600,
		});
		// Blank password field on edit: the stored one is kept, never overwritten.
		expect(mockInvoke).not.toHaveBeenCalledWith("set_remote_connection_password", expect.anything());
	});
});
