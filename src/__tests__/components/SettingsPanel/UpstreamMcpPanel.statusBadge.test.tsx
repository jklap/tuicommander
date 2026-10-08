import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

// Regression coverage for the status-indicator swap in UpstreamMcpPanel.tsx:
// the visible-label case now goes through the shared `ConnectionStatusBadge`
// (matching RemoteServersTab.tsx's use of it for the equivalent connection-row
// case) instead of a bespoke inline dot+span; the compact "server is already
// disabled" case still falls back to a bare dot with a tooltip only, so it
// doesn't duplicate the separate "Disabled" pill the row already shows.

const statusBox = vi.hoisted(() => ({
	serverEnabled: true,
	upstreams: [] as unknown[],
}));

vi.mock("../../../transport", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../../transport")>();
	return {
		...actual,
		rpc: (async (command: string, ...rest: unknown[]) => {
			if (command === "load_mcp_upstreams") {
				return {
					servers: [
						{
							id: "srv1",
							name: "example",
							transport: { type: "http", url: "https://example.com/mcp" },
							enabled: statusBox.serverEnabled,
							timeout_secs: 30,
						},
					],
				};
			}
			if (command === "get_mcp_upstream_status") return { upstreams: statusBox.upstreams };
			// biome-ignore lint/suspicious/noExplicitAny: forwarding to the real overloaded rpc()
			return (actual.rpc as any)(command, ...rest);
		}) as typeof actual.rpc,
	};
});

import { UpstreamMcpPanel } from "../../../components/SettingsPanel/tabs/services/UpstreamMcpPanel";

async function flushMicrotasks(): Promise<void> {
	await new Promise<void>((resolve) => setImmediate(resolve));
}

function statusEntry(status: string) {
	return {
		name: "example",
		status,
		transport: { type: "http", url: "https://example.com/mcp" },
		tool_count: 0,
		tools: [],
		metrics: { call_count: 0, error_count: 0, last_latency_ms: 0 },
	};
}

describe("UpstreamMcpPanel — status indicator", () => {
	beforeEach(() => {
		statusBox.serverEnabled = true;
		statusBox.upstreams = [];
	});

	afterEach(async () => {
		vi.clearAllTimers();
		await flushMicrotasks();
	});

	it("shows the shared ConnectionStatusBadge's visible label when the server is enabled and connected", async () => {
		statusBox.upstreams = [statusEntry("ready")];
		const { getByText, unmount } = render(() => <UpstreamMcpPanel />);
		await flushMicrotasks();

		expect(getByText("example")).toBeTruthy();
		expect(getByText("Connected")).toBeTruthy();
		unmount();
	});

	it("shows the visible label for an in-progress state too (Connecting…)", async () => {
		statusBox.upstreams = [statusEntry("connecting")];
		const { getByText, unmount } = render(() => <UpstreamMcpPanel />);
		await flushMicrotasks();

		expect(getByText("Connecting…")).toBeTruthy();
		unmount();
	});

	it("falls back to a dot-only indicator (no duplicate visible label) when the server is disabled", async () => {
		statusBox.serverEnabled = false;
		statusBox.upstreams = [statusEntry("disabled")];
		const { getAllByText, container, unmount } = render(() => <UpstreamMcpPanel />);
		await flushMicrotasks();

		// Exactly one "Disabled" text node — the row's separate Disabled pill.
		// A second one would mean ConnectionStatusBadge's visible-label branch
		// wrongly fired for the compact/disabled case too.
		expect(getAllByText("Disabled")).toHaveLength(1);
		// The dot itself still carries the status as a tooltip.
		expect(container.querySelector('[title="Disabled"]')).toBeTruthy();
		unmount();
	});
});
