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
		expect(getByText(/No remote connections configured/)).toBeTruthy();
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
			transport: { type: "Direct", url: "http://10.0.0.5:9877", tls_fingerprint: null },
		});
	});

	it("editing an existing remote connection shows its Name field pre-filled (bug fix: rename without delete+recreate)", async () => {
		await remoteConnectionsStore.addConnection({
			id: "edit1",
			name: "old-name",
			transport: { type: "Direct", url: "http://host:9876", tls_fingerprint: null },
			auth_username: "",
			enabled: true,
		});
		mockInvoke.mockClear();
		const { getByText, getByDisplayValue } = render(() => <RemoteServersTab />);
		await flushMicrotasks();

		fireEvent.click(getByText("Edit"));
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
			transport: { type: "Direct", url: "http://host:9876", tls_fingerprint: null },
			auth_username: "",
			enabled: true,
		});

		const { getByText } = render(() => <RemoteServersTab />);
		await flushMicrotasks();

		expect(getByText("prod tunnel")).toBeTruthy();
		expect(getByText("dev-box")).toBeTruthy();
	});
});
