import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { TunnelEditorModal } from "../../components/TunnelsPanel/TunnelEditorModal";
import { __resetModalStackForTest } from "../../stores/modalStack";
import type { TunnelProfile } from "../../stores/tunnels";
import { mockInvoke } from "../mocks/tauri";

function makeProfile(overrides: Partial<TunnelProfile> = {}): TunnelProfile {
	return {
		id: "existing-1",
		name: "prod db tunnel",
		host: "prod.example.test",
		port: 2222,
		user: "deploy",
		identity_file: "~/.ssh/id_ed25519",
		forwards: [],
		options: { server_alive_interval: 15, server_alive_count_max: 3, strict_host_key_checking: "Yes" },
		auto_connect: true,
		...overrides,
	};
}

async function flushMicrotasks(): Promise<void> {
	await new Promise<void>((resolve) => setImmediate(resolve));
}

/** Fields are laid out as sibling <label>/<input> pairs inside a shared group div. */
function getFieldInput(container: HTMLElement, labelText: string): HTMLInputElement {
	const label = Array.from(container.querySelectorAll("label")).find((el) => el.textContent === labelText);
	if (!label?.parentElement) throw new Error(`Could not find a field group for label "${labelText}"`);
	const input = label.parentElement.querySelector("input");
	if (!input) throw new Error(`Could not find an <input> near label "${labelText}"`);
	return input as HTMLInputElement;
}

describe("TunnelEditorModal", () => {
	beforeEach(() => {
		__resetModalStackForTest();
		mockInvoke.mockReset();
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_ssh_config_hosts") return Promise.resolve([]);
			if (cmd === "list_ssh_agent_keys") return Promise.resolve({ keys: [], agent_type: "" });
			return Promise.resolve(undefined);
		});
	});

	afterEach(async () => {
		cleanup();
		await flushMicrotasks();
	});

	it("renders empty required fields under the 'New Tunnel' heading when no profile is passed", async () => {
		const { getByText, container } = render(() => <TunnelEditorModal onClose={vi.fn()} />);
		await flushMicrotasks();

		expect(getByText("New Tunnel")).toBeTruthy();
		expect(getFieldInput(container, "Name").value).toBe("");
		expect(getFieldInput(container, "Host").value).toBe("");
		expect(getFieldInput(container, "User").value).toBe("");
	});

	it("renders 'Edit Tunnel' pre-filled from the given profile", async () => {
		const profile = makeProfile();
		const { getByText, getByDisplayValue } = render(() => <TunnelEditorModal profile={profile} onClose={vi.fn()} />);
		await flushMicrotasks();

		expect(getByText("Edit Tunnel")).toBeTruthy();
		expect(getByDisplayValue("prod db tunnel")).toBeTruthy();
		expect(getByDisplayValue("prod.example.test")).toBeTruthy();
		expect(getByDisplayValue("2222")).toBeTruthy();
		expect(getByDisplayValue("deploy")).toBeTruthy();
		expect(getByDisplayValue("~/.ssh/id_ed25519")).toBeTruthy();
	});

	it("Save with blank required fields shows a validation error and never calls save_tunnel_profile", async () => {
		const onClose = vi.fn();
		const { getByText } = render(() => <TunnelEditorModal onClose={onClose} />);
		await flushMicrotasks();

		fireEvent.click(getByText("Save"));
		await flushMicrotasks();

		expect(getByText("Name, host, and user are required.")).toBeTruthy();
		expect(mockInvoke).not.toHaveBeenCalledWith("save_tunnel_profile", expect.anything());
		expect(onClose).not.toHaveBeenCalled();
	});

	it("Save creates a new profile via save_tunnel_profile (no id) and closes", async () => {
		const onClose = vi.fn();
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_ssh_config_hosts") return Promise.resolve([]);
			if (cmd === "list_ssh_agent_keys") return Promise.resolve({ keys: [], agent_type: "" });
			if (cmd === "save_tunnel_profile") return Promise.resolve();
			if (cmd === "list_tunnel_profiles") return Promise.resolve([]);
			return Promise.resolve(undefined);
		});
		const { getByText, container } = render(() => <TunnelEditorModal onClose={onClose} />);
		await flushMicrotasks();

		fireEvent.input(getFieldInput(container, "Name"), { target: { value: "new tunnel" } });
		fireEvent.input(getFieldInput(container, "Host"), { target: { value: "host.example.test" } });
		fireEvent.input(getFieldInput(container, "User"), { target: { value: "boss" } });
		fireEvent.click(getByText("Save"));
		await flushMicrotasks();

		expect(mockInvoke).toHaveBeenCalledWith("save_tunnel_profile", {
			profile: {
				name: "new tunnel",
				host: "host.example.test",
				port: 22,
				user: "boss",
				identity_file: null,
				forwards: [],
				options: { server_alive_interval: 15, server_alive_count_max: 3, strict_host_key_checking: "Yes" },
				auto_connect: false,
			},
		});
		expect(mockInvoke).toHaveBeenCalledWith("list_tunnel_profiles");
		expect(onClose).toHaveBeenCalledTimes(1);
	});

	it("Save on an existing profile updates via save_tunnel_profile with its id and closes", async () => {
		const onClose = vi.fn();
		const profile = makeProfile();
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_ssh_config_hosts") return Promise.resolve([]);
			if (cmd === "list_ssh_agent_keys") return Promise.resolve({ keys: [], agent_type: "" });
			if (cmd === "save_tunnel_profile") return Promise.resolve();
			if (cmd === "list_tunnel_profiles") return Promise.resolve([profile]);
			return Promise.resolve(undefined);
		});
		const { getByText, container } = render(() => <TunnelEditorModal profile={profile} onClose={onClose} />);
		await flushMicrotasks();

		fireEvent.input(getFieldInput(container, "Name"), { target: { value: "renamed tunnel" } });
		fireEvent.click(getByText("Save"));
		await flushMicrotasks();

		expect(mockInvoke).toHaveBeenCalledWith("save_tunnel_profile", {
			profile: {
				id: profile.id,
				name: "renamed tunnel",
				host: profile.host,
				port: profile.port,
				user: profile.user,
				identity_file: profile.identity_file,
				forwards: [],
				options: profile.options,
				auto_connect: profile.auto_connect,
			},
		});
		expect(onClose).toHaveBeenCalledTimes(1);
	});

	it("Cancel closes without saving", async () => {
		const onClose = vi.fn();
		const { getByText } = render(() => <TunnelEditorModal onClose={onClose} />);
		await flushMicrotasks();

		fireEvent.click(getByText("Cancel"));

		expect(onClose).toHaveBeenCalledTimes(1);
		expect(mockInvoke).not.toHaveBeenCalledWith("save_tunnel_profile", expect.anything());
	});

	it("shows the error message and does not close when save_tunnel_profile fails", async () => {
		const onClose = vi.fn();
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_ssh_config_hosts") return Promise.resolve([]);
			if (cmd === "list_ssh_agent_keys") return Promise.resolve({ keys: [], agent_type: "" });
			if (cmd === "save_tunnel_profile") return Promise.reject(new Error("disk full"));
			return Promise.resolve(undefined);
		});
		const { getByText, container } = render(() => <TunnelEditorModal onClose={onClose} />);
		await flushMicrotasks();

		fireEvent.input(getFieldInput(container, "Name"), { target: { value: "x" } });
		fireEvent.input(getFieldInput(container, "Host"), { target: { value: "h" } });
		fireEvent.input(getFieldInput(container, "User"), { target: { value: "u" } });
		fireEvent.click(getByText("Save"));
		await flushMicrotasks();

		expect(getByText("disk full")).toBeTruthy();
		expect(onClose).not.toHaveBeenCalled();
	});
});
