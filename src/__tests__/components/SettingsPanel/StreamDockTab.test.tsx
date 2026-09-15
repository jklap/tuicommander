import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
vi.mock("../../../transport", () => ({ rpc: vi.fn() }));
vi.mock("../../../utils/updateAppConfig", () => ({ updateAppConfig: vi.fn() }));
vi.mock("../../../stores/terminals", () => ({
	terminalsStore: { state: { terminals: {} } },
}));

import { StreamDockTab } from "../../../components/SettingsPanel/tabs/StreamDockTab";
import { terminalsStore } from "../../../stores/terminals";
import { rpc } from "../../../transport";
import { updateAppConfig } from "../../../utils/updateAppConfig";

const DEFAULT_CONFIG = {
	streamdock: {
		enabled: false,
		device_serial: null,
		screen_brightness: 70,
		led_brightness: 40,
		pinned_sessions: [],
	},
};

const DEFAULT_STATUS = {
	enabled: false,
	running: false,
	device: null,
	last_error: null,
	restarts: 0,
};

describe("StreamDockTab", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.mocked(rpc).mockImplementation((command: string) => {
			if (command === "load_config") return Promise.resolve(DEFAULT_CONFIG);
			if (command === "streamdock_status") return Promise.resolve(DEFAULT_STATUS);
			if (command === "streamdock_list_devices") return Promise.resolve([]);
			return Promise.resolve(undefined);
		});
		terminalsStore.state.terminals = {};
	});

	afterEach(() => {
		vi.clearAllTimers();
		vi.useRealTimers();
		vi.clearAllMocks();
	});

	it("loads config/status/devices on mount and shows the disabled status", async () => {
		const view = render(() => <StreamDockTab />);
		await vi.advanceTimersByTimeAsync(0);

		expect(rpc).toHaveBeenCalledWith("load_config");
		expect(rpc).toHaveBeenCalledWith("streamdock_status");
		expect(rpc).toHaveBeenCalledWith("streamdock_list_devices");
		expect(view.getByText("Disabled")).not.toBeNull();
		expect(view.getByText("No StreamDock device currently detected.")).not.toBeNull();

		view.unmount();
	});

	it("re-polls status every 2 seconds while mounted and stops after unmount", async () => {
		const view = render(() => <StreamDockTab />);
		await vi.advanceTimersByTimeAsync(0);
		const statusCalls = () => vi.mocked(rpc).mock.calls.filter(([command]) => command === "streamdock_status").length;
		expect(statusCalls()).toBe(1);

		await vi.advanceTimersByTimeAsync(2000);
		expect(statusCalls()).toBe(2);

		view.unmount();
		await vi.advanceTimersByTimeAsync(4000);
		expect(statusCalls()).toBe(2);
	});

	it("shows a connected status with the device name once running", async () => {
		vi.mocked(rpc).mockImplementation((command: string) => {
			if (command === "load_config") return Promise.resolve(DEFAULT_CONFIG);
			if (command === "streamdock_status")
				return Promise.resolve({
					enabled: true,
					running: true,
					device: "StreamDock M18",
					last_error: null,
					restarts: 2,
				});
			if (command === "streamdock_list_devices") return Promise.resolve([]);
			return Promise.resolve(undefined);
		});

		const view = render(() => <StreamDockTab />);
		await vi.advanceTimersByTimeAsync(0);

		expect(view.getByText("Connected — StreamDock M18")).not.toBeNull();
		expect(view.getByText("Reconnected 2 time(s) since enabled.")).not.toBeNull();

		view.unmount();
	});

	// A last_error present alongside enabled:true (e.g. "device is claimed by
	// another application") must surface the error text as the status label,
	// not silently fall through to a generic "waiting" message — this is the
	// one path a user actually needs to see to unblock themselves.
	it("surfaces last_error as the status label when the device failed to connect", async () => {
		vi.mocked(rpc).mockImplementation((command: string) => {
			if (command === "load_config") return Promise.resolve(DEFAULT_CONFIG);
			if (command === "streamdock_status")
				return Promise.resolve({
					enabled: true,
					running: false,
					device: null,
					last_error: "device is claimed by another application (quit Mirabox Creator)",
					restarts: 0,
				});
			if (command === "streamdock_list_devices") return Promise.resolve([]);
			return Promise.resolve(undefined);
		});

		const view = render(() => <StreamDockTab />);
		await vi.advanceTimersByTimeAsync(0);

		expect(view.getByText("device is claimed by another application (quit Mirabox Creator)")).not.toBeNull();

		view.unmount();
	});

	it("lists live sessions as pinnable and toggling one saves the pinned_sessions list", async () => {
		terminalsStore.state.terminals = {
			"term-1": { sessionId: "sess-1", alias: "tc-1", name: "tc-1" },
		} as unknown as typeof terminalsStore.state.terminals;
		vi.mocked(updateAppConfig).mockImplementation(async (mutate: (c: typeof DEFAULT_CONFIG) => void) => {
			const next = structuredClone(DEFAULT_CONFIG);
			mutate(next);
			return next;
		});

		const view = render(() => <StreamDockTab />);
		await vi.advanceTimersByTimeAsync(0);

		expect(view.queryByText("No live sessions to pin right now.")).toBeNull();
		const label = view.getByText("tc-1");
		const toggle = label.parentElement?.querySelector("input[type='checkbox']") as HTMLInputElement;
		expect(toggle).not.toBeNull();
		toggle.click();
		await vi.advanceTimersByTimeAsync(0);

		expect(updateAppConfig).toHaveBeenCalled();

		view.unmount();
	});

	it("shows the empty-sessions fallback when nothing is live", async () => {
		const view = render(() => <StreamDockTab />);
		await vi.advanceTimersByTimeAsync(0);

		expect(view.getByText("No live sessions to pin right now.")).not.toBeNull();

		view.unmount();
	});
});
