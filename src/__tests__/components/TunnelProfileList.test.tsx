import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { TunnelProfileList } from "../../components/TunnelsPanel/TunnelProfileList";
import type { TunnelProfile } from "../../stores/tunnels";
import { tunnelsStore } from "../../stores/tunnels";
import { mockInvoke } from "../mocks/tauri";

function makeProfile(overrides: Partial<TunnelProfile> = {}): TunnelProfile {
	return {
		id: "p1",
		name: "dev tunnel",
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

async function flushMicrotasks(): Promise<void> {
	await new Promise<void>((resolve) => setImmediate(resolve));
}

describe("TunnelProfileList", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "list_tunnel_profiles") return Promise.resolve([makeProfile()]);
			if (cmd === "list_active_tunnels") return Promise.resolve([]);
			return Promise.resolve(undefined);
		});
	});

	afterEach(async () => {
		cleanup();
		await flushMicrotasks();
	});

	it("does not render an Edit button when onEdit is omitted (Tunnels overlay usage)", async () => {
		await tunnelsStore.refreshProfiles();
		await tunnelsStore.refreshActiveTunnels();
		const { queryByText } = render(() => <TunnelProfileList />);
		await flushMicrotasks();

		expect(queryByText("Edit")).toBeNull();
	});

	it("renders an Edit button that calls onEdit with the profile when provided (Settings usage)", async () => {
		await tunnelsStore.refreshProfiles();
		await tunnelsStore.refreshActiveTunnels();
		const onEdit = vi.fn();
		const { getByText } = render(() => <TunnelProfileList onEdit={onEdit} />);
		await flushMicrotasks();

		fireEvent.click(getByText("Edit"));
		expect(onEdit).toHaveBeenCalledWith(expect.objectContaining({ id: "p1" }));
	});
});
