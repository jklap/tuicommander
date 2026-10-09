import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { actions, statuses } = vi.hoisted(() => ({
	actions: {
		addConnection: vi.fn<(connection: unknown) => Promise<void>>(() => Promise.resolve()),
		setPassword: vi.fn<(id: string, password: string) => Promise<void>>(() => Promise.resolve()),
		hasPassword: vi.fn(() => Promise.resolve(false)),
		disconnect: vi.fn(() => Promise.resolve()),
		testConnection: vi.fn(() => Promise.resolve({ type: "Reachable" })),
	},
	statuses: {} as Record<string, { status: string }>,
}));

vi.mock("../../../stores/remoteConnections", () => ({
	remoteConnectionsStore: {
		...actions,
		getConnectionState: (id: string) => statuses[id],
	},
}));
vi.mock("../../../stores/tunnels", () => ({
	tunnelsStore: { createProfile: vi.fn(), updateProfile: vi.fn() },
}));
vi.mock("../../../invoke", () => ({ invoke: vi.fn(() => Promise.resolve(undefined)) }));

import {
	prefillFromDiscoveredHost,
	RemoteConnectionEditor,
} from "../../../components/SettingsPanel/tabs/services/RemoteConnectionEditor";

/** Sibling `<label>`/control pairs, as in RemoteServersTab.render.test.tsx. */
function field(container: HTMLElement, labelText: string): HTMLInputElement | HTMLSelectElement {
	const label = Array.from(container.querySelectorAll("label")).find((el) => el.textContent === labelText);
	const control = label?.parentElement?.querySelector("input, select");
	if (!control) throw new Error(`no control for label "${labelText}"`);
	return control as HTMLInputElement | HTMLSelectElement;
}

const sshConnection = {
	id: "machine-1",
	name: "Build host",
	transport: {
		type: "Ssh" as const,
		ssh: {
			host: "builder.local",
			port: 22,
			user: "dev",
			identity_file: null,
			server_alive_interval: 15,
			server_alive_count_max: 3,
			strict_host_key_checking: "Yes" as const,
			compression: false,
		},
		remote_daemon_port: 9876,
	},
	auth_username: "tuic",
	enabled: true,
	deploy: "installed" as const,
	survive_secs: 2700,
	auto_update: true,
};

describe("RemoteConnectionEditor", () => {
	beforeEach(() => {
		for (const action of Object.values(actions)) action.mockClear();
		for (const key of Object.keys(statuses)) delete statuses[key];
	});

	afterEach(() => cleanup());

	it("a discovered host prefills a Remote Server — SSH connection with its name, host, user and port", () => {
		const target = {
			kind: "new" as const,
			prefill: prefillFromDiscoveredHost({ host: "vps", target: "vps", user: "boss", port: 2222, source: "config" }),
		};
		const { container } = render(() => <RemoteConnectionEditor target={target} onClose={vi.fn()} />);

		expect(field(container, "Kind").value).toBe("RemoteSsh");
		expect(field(container, "Name").value).toBe("vps");
		expect(field(container, "Host").value).toBe("vps");
		expect(field(container, "User").value).toBe("boss");
		expect(field(container, "Port").value).toBe("2222");
	});

	it("offers auto-update per connection, off for a new machine, and saves the choice", async () => {
		const onClose = vi.fn();
		const { container, getByLabelText, getByText } = render(() => (
			<RemoteConnectionEditor target={{ kind: "new" }} onClose={onClose} />
		));
		fireEvent.change(field(container, "Kind"), { target: { value: "RemoteDirect" } });
		const toggle = getByLabelText("Auto-update remote daemons") as HTMLInputElement;
		expect(toggle.checked).toBe(false);
		fireEvent.input(field(container, "Name"), { target: { value: "Builder" } });
		fireEvent.input(field(container, "URL"), { target: { value: "http://builder.local:9877" } });
		fireEvent.click(toggle);
		fireEvent.click(getByText("Save"));

		await waitFor(() => expect(onClose).toHaveBeenCalled());
		expect(actions.addConnection).toHaveBeenCalledWith(expect.objectContaining({ auto_update: true, deploy: "never" }));
	});

	it("persists deployment mode and survive minutes for an SSH machine", async () => {
		const { container, getByText } = render(() => (
			<RemoteConnectionEditor target={{ kind: "new" }} onClose={vi.fn()} />
		));
		fireEvent.change(field(container, "Kind"), { target: { value: "RemoteSsh" } });
		fireEvent.input(field(container, "Name"), { target: { value: "Builder" } });
		fireEvent.input(field(container, "Host"), { target: { value: "builder.local" } });
		fireEvent.input(field(container, "User"), { target: { value: "dev" } });
		fireEvent.change(field(container, "Deployment"), { target: { value: "on_connect" } });
		fireEvent.input(field(container, "Keep ephemeral daemon alive (minutes)"), { target: { value: "45" } });
		fireEvent.click(getByText("Save"));

		await waitFor(() => expect(actions.addConnection).toHaveBeenCalledTimes(1));
		expect(actions.addConnection.mock.calls[0][0]).toMatchObject({
			name: "Builder",
			deploy: "on_connect",
			survive_secs: 2700,
		});
	});

	it("saves the SSH provisioning options only when set, and keeps the shape unchanged otherwise", async () => {
		const fill = (container: HTMLElement) => {
			fireEvent.change(field(container, "Kind"), { target: { value: "RemoteSsh" } });
			fireEvent.input(field(container, "Name"), { target: { value: "Builder" } });
			fireEvent.input(field(container, "Host"), { target: { value: "builder.local" } });
			fireEvent.input(field(container, "User"), { target: { value: "dev" } });
		};
		const first = render(() => <RemoteConnectionEditor target={{ kind: "new" }} onClose={vi.fn()} />);
		fill(first.container);
		expect(first.queryByLabelText("Leave it running on disconnect")).toBeNull();
		fireEvent.click(first.getByText("Save"));
		await waitFor(() => expect(actions.addConnection).toHaveBeenCalledTimes(1));
		const plain = (actions.addConnection.mock.calls[0][0] as { transport: Record<string, unknown> }).transport;
		expect(Object.keys(plain).sort()).toEqual(["remote_daemon_port", "ssh", "type"]);
		cleanup();

		const second = render(() => <RemoteConnectionEditor target={{ kind: "new" }} onClose={vi.fn()} />);
		fill(second.container);
		fireEvent.input(field(second.container, "Instance ID (optional)"), { target: { value: "dev-box" } });
		fireEvent.click(second.getByLabelText("Offer to start the remote daemon if it is not running"));
		fireEvent.click(second.getByLabelText("Leave it running on disconnect"));
		fireEvent.click(second.getByText("Save"));
		await waitFor(() => expect(actions.addConnection).toHaveBeenCalledTimes(2));
		expect(actions.addConnection.mock.calls[1][0]).toMatchObject({
			transport: {
				type: "Ssh",
				start_if_not_running: true,
				leave_running_on_disconnect: true,
				instance_id: "dev-box",
			},
		});
	});

	it("refuses an Instance ID tuic-remote would not accept", async () => {
		const { container, getByText } = render(() => (
			<RemoteConnectionEditor target={{ kind: "new" }} onClose={vi.fn()} />
		));
		fireEvent.change(field(container, "Kind"), { target: { value: "RemoteSsh" } });
		fireEvent.input(field(container, "Name"), { target: { value: "Builder" } });
		fireEvent.input(field(container, "Host"), { target: { value: "builder.local" } });
		fireEvent.input(field(container, "User"), { target: { value: "dev" } });
		fireEvent.input(field(container, "Instance ID (optional)"), { target: { value: "dev; reboot" } });
		fireEvent.click(getByText("Save"));
		await waitFor(() => expect(getByText(/Instance ID must be a lowercase DNS label/)).toBeTruthy());
		expect(actions.addConnection).not.toHaveBeenCalled();
	});

	it("editing a live connection disconnects it first and keeps its fields; a saved Yes becomes accept-new", async () => {
		statuses["machine-1"] = { status: "connected" };
		const onClose = vi.fn();
		const { container, getByText } = render(() => (
			<RemoteConnectionEditor target={{ kind: "edit-connection", connection: sshConnection }} onClose={onClose} />
		));
		expect((field(container, "Kind") as HTMLSelectElement).disabled).toBe(true);
		expect(field(container, "Deployment").value).toBe("installed");
		expect(field(container, "Keep ephemeral daemon alive (minutes)").value).toBe("45");
		fireEvent.click(getByText("Save"));

		await waitFor(() => expect(onClose).toHaveBeenCalled());
		expect(actions.disconnect).toHaveBeenCalledWith("machine-1");
		expect(actions.disconnect.mock.invocationCallOrder[0]).toBeLessThan(
			actions.addConnection.mock.invocationCallOrder[0],
		);
		expect(actions.addConnection.mock.calls[0][0]).toMatchObject({
			id: "machine-1",
			auth_username: "tuic",
			deploy: "installed",
			survive_secs: 2700,
			auto_update: true,
			transport: {
				type: "Ssh",
				remote_daemon_port: 9876,
				ssh: { host: "builder.local", compression: false, strict_host_key_checking: "AcceptNew" },
			},
		});
		expect(actions.setPassword).not.toHaveBeenCalled();
	});

	describe("a pinned Direct certificate", () => {
		const pin = "ab".repeat(32);
		const pinned = {
			id: "machine-2",
			name: "Self-signed box",
			transport: { type: "Direct" as const, url: "https://box:9877", tls_fingerprint: pin },
			auth_username: "tuic",
			enabled: true,
			deploy: "never" as const,
			survive_secs: 1800,
		};
		const edit = () => {
			const onClose = vi.fn();
			const view = render(() => (
				<RemoteConnectionEditor target={{ kind: "edit-connection", connection: pinned }} onClose={onClose} />
			));
			return { ...view, onClose };
		};
		const savedTransport = () => (actions.addConnection.mock.calls[0][0] as { transport: unknown }).transport;

		it("is shown and survives a save that edits something else", async () => {
			const { container, getByText, onClose } = edit();
			expect(container.textContent).toContain(pin);
			fireEvent.input(field(container, "Name"), { target: { value: "Renamed" } });
			fireEvent.click(getByText("Save"));
			await waitFor(() => expect(onClose).toHaveBeenCalled());
			expect(savedTransport()).toEqual({ type: "Direct", url: "https://box:9877", tls_fingerprint: pin });
		});

		it("is dropped when the URL changes: a pin belongs to its target", async () => {
			const { container, getByText, onClose } = edit();
			fireEvent.input(field(container, "URL"), { target: { value: "https://other:9877" } });
			expect(container.textContent).not.toContain(pin);
			fireEvent.click(getByText("Save"));
			await waitFor(() => expect(onClose).toHaveBeenCalled());
			expect(savedTransport()).toEqual({ type: "Direct", url: "https://other:9877" });
		});

		it("Forget pinned certificate removes it so the next Connect asks again", async () => {
			const { getByText, onClose } = edit();
			fireEvent.click(getByText("Forget pinned certificate"));
			fireEvent.click(getByText("Save"));
			await waitFor(() => expect(onClose).toHaveBeenCalled());
			expect(savedTransport()).toEqual({ type: "Direct", url: "https://box:9877" });
		});
	});

	it("Clear stored password forgets it through the vault's empty-password request", async () => {
		actions.hasPassword.mockResolvedValueOnce(true);
		const { findByText } = render(() => (
			<RemoteConnectionEditor target={{ kind: "edit-connection", connection: sshConnection }} onClose={vi.fn()} />
		));
		fireEvent.click(await findByText("Clear stored password"));

		await waitFor(() => expect(actions.setPassword).toHaveBeenCalledWith("machine-1", ""));
	});

	it("Test Connection sends the form's transport and the typed credentials", async () => {
		const { container, getByText, findByText } = render(() => (
			<RemoteConnectionEditor target={{ kind: "new" }} onClose={vi.fn()} />
		));
		fireEvent.change(field(container, "Kind"), { target: { value: "RemoteDirect" } });
		fireEvent.input(field(container, "URL"), { target: { value: "http://h:9877" } });
		fireEvent.input(field(container, "Auth username (optional)"), { target: { value: "boss" } });
		fireEvent.input(field(container, "Auth password (optional)"), { target: { value: "pw" } });
		fireEvent.click(getByText("Test Connection"));

		expect(await findByText("Reachable")).toBeTruthy();
		expect(actions.testConnection).toHaveBeenCalledWith({
			transport: { type: "Direct", url: "http://h:9877" },
			auth_username: "boss",
			password: "pw",
		});
	});

	it("a protected daemon tested with no password says a password is required, not 'not configured'", async () => {
		actions.testConnection.mockResolvedValueOnce({ type: "PasswordRequired" });
		const { container, getByText, findByText, queryByText } = render(() => (
			<RemoteConnectionEditor target={{ kind: "new" }} onClose={vi.fn()} />
		));
		fireEvent.change(field(container, "Kind"), { target: { value: "RemoteDirect" } });
		fireEvent.input(field(container, "URL"), { target: { value: "http://h:9877" } });
		fireEvent.click(getByText("Test Connection"));

		expect(await findByText(/requires a password/)).toBeTruthy();
		expect(queryByText(/not configured/)).toBeNull();
	});
});
