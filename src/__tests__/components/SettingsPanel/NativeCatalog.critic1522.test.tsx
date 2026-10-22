import { render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	rpc: vi.fn(),
}));

import { LocalMcpPanel } from "../../../components/SettingsPanel/tabs/services/LocalMcpPanel";
import { rpc } from "../../../transport";

afterEach(() => {
	vi.clearAllTimers();
	vi.useRealTimers();
	vi.clearAllMocks();
});

// Catches: Settings snapshots the initial registry and misses tools supplied
// by a restarted backend on a subsequent status poll.
it("refreshes native switches when a later backend poll registers a tool", async () => {
	vi.useFakeTimers();
	let registered = false;
	vi.mocked(rpc).mockImplementation((command: string) => {
		if (command === "get_mcp_status") {
			return Promise.resolve({
				enabled: true,
				running: true,
				native_capabilities: registered
					? [{ name: "telegram", title: "telegram", description: "Telegram backend description", optional: true }]
					: [],
			});
		}
		if (command === "load_config") {
			return Promise.resolve({
				mcp_capabilities: { denied: ["telegram"], migration_unknown_tools: [] },
				collapse_tools: false,
			});
		}
		return Promise.resolve({ connected: false });
	});
	const view = render(() => <LocalMcpPanel />);
	try {
		await vi.advanceTimersByTimeAsync(0);
		expect(view.queryByText("telegram")).toBeNull();
		registered = true;
		await vi.advanceTimersByTimeAsync(3000);
		const toggle = view.getByRole("checkbox", { name: "telegram" }) as HTMLInputElement;
		expect(toggle.checked).toBe(false);
		expect(view.getByText("Telegram backend description")).toBeDefined();
	} finally {
		view.unmount();
	}
});
