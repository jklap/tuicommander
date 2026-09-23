import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import { SshConnectionFields } from "../../../components/shared/SshConnectionFields";
import type { SshConnectionParams } from "../../../stores/tunnels";
import { mockInvoke } from "../../mocks/tauri";

function defaultSsh(overrides: Partial<SshConnectionParams> = {}): SshConnectionParams {
	return {
		host: "",
		port: 22,
		user: "",
		identity_file: null,
		server_alive_interval: 15,
		server_alive_count_max: 3,
		strict_host_key_checking: "Yes",
		...overrides,
	};
}

async function flushMicrotasks(): Promise<void> {
	await new Promise<void>((resolve) => setImmediate(resolve));
}

describe("SshConnectionFields", () => {
	beforeEach(() => {
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

	it("renders all fields from the given value, including the new ServerAliveCountMax field", async () => {
		const value = defaultSsh({ host: "h.example.test", port: 2222, user: "boss", server_alive_count_max: 7 });
		const { getByDisplayValue, getByText } = render(() => <SshConnectionFields value={value} onChange={vi.fn()} />);
		await flushMicrotasks();

		expect(getByDisplayValue("h.example.test")).toBeTruthy();
		expect(getByDisplayValue("2222")).toBeTruthy();
		expect(getByDisplayValue("boss")).toBeTruthy();
		expect(getByText("ServerAliveCountMax")).toBeTruthy();
		expect(getByDisplayValue("7")).toBeTruthy();
	});

	it("calls onChange with a patch when the host input changes", async () => {
		const onChange = vi.fn();
		const { container } = render(() => <SshConnectionFields value={defaultSsh()} onChange={onChange} />);
		await flushMicrotasks();

		const label = Array.from(container.querySelectorAll("label")).find((el) => el.textContent === "Host");
		const input = label?.parentElement?.querySelector("input") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "new-host" } });

		expect(onChange).toHaveBeenCalledWith({ host: "new-host" });
	});

	it("renders the SSH agent's detected keys with each key's fingerprint (previously fetched but never rendered)", async () => {
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_ssh_config_hosts") return Promise.resolve([]);
			if (cmd === "list_ssh_agent_keys")
				return Promise.resolve({
					agent_type: "ssh-agent",
					keys: [{ fingerprint: "SHA256:abc123", comment: "boss@laptop", key_type: "ED25519" }],
				});
			return Promise.resolve(undefined);
		});
		const { findByText } = render(() => <SshConnectionFields value={defaultSsh()} onChange={vi.fn()} />);

		expect(await findByText(/SHA256:abc123/)).toBeTruthy();
	});

	it("changing ServerAliveCountMax calls onChange with the parsed number", async () => {
		const onChange = vi.fn();
		const { container } = render(() => <SshConnectionFields value={defaultSsh()} onChange={onChange} />);
		await flushMicrotasks();

		const label = Array.from(container.querySelectorAll("label")).find(
			(el) => el.textContent === "ServerAliveCountMax",
		);
		const input = label?.parentElement?.querySelector("input") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "5" } });

		expect(onChange).toHaveBeenCalledWith({ server_alive_count_max: 5 });
	});
});
