import { beforeEach, describe, expect, it, vi } from "vitest";

type Reply = { ok: boolean; result?: any; error?: string };
const PANEL = "chrome-extension://ext/panel.html";
const calls: { action: string; sessionId?: string }[] = [];
const serversOf: Record<string, string[]> = { A: ["http://localhost:3001/"], B: ["http://localhost:3002/"] };

vi.mock("../../../browser-extension/design-mode/native.js", () => ({
	connectNative: () => {},
	nativeRequest: async (payload: { action: string; sessionId?: string }) => {
		calls.push({ action: payload.action, sessionId: payload.sessionId });
		if (payload.action === "hello") return { endpoint: "test" };
		if (payload.action === "list_agents")
			return ["A", "B"].map((sessionId) => ({ sessionId, agentType: "claude", repoPath: "/repo", label: sessionId }));
		if (payload.action === "mode_status") return [];
		if (payload.action === "list_dev_servers")
			return { servers: (serversOf[payload.sessionId ?? ""] ?? []).map((url) => ({ url })), canStart: false };
		throw new Error(`unexpected ${payload.action}`);
	},
}));

async function boot() {
	let listener: (m: unknown, s: unknown, r: (x: Reply) => void) => boolean | undefined = () => undefined;
	const store: Record<string, unknown> = {};
	const tabsUpdate = vi.fn(async () => {});
	vi.stubGlobal("fetch", async () => ({ ok: true, json: async () => ({ repoPath: "/repo", sessionId: "A" }) }));
	vi.stubGlobal("chrome", {
		runtime: {
			getURL: (p: string) => `chrome-extension://ext/${p}`,
			sendMessage: async () => {},
			onMessage: { addListener: (fn: typeof listener) => { listener = fn; } },
		},
		sidePanel: { setPanelBehavior: () => {} },
		storage: { local: { get: async () => ({ ...store }), set: async (v: Record<string, unknown>) => { Object.assign(store, v); } } },
		tabs: {
			query: async () => [{ id: 7 }],
			sendMessage: async () => {},
			update: tabsUpdate,
			onActivated: { addListener: () => {} },
			onUpdated: { addListener: () => {} },
			onRemoved: { addListener: () => {} },
		},
	});
	await import("../../../browser-extension/design-mode/worker.js");
	const send = (message: object) =>
		new Promise<Reply>((resolve) => listener(message, { url: PANEL }, resolve));
	return { send, tabsUpdate };
}

describe("Design Mode worker dev-server wiring", () => {
	beforeEach(() => {
		vi.resetModules();
		calls.length = 0;
	});

	// Catches: the dropdown pick changing the recipient while the list stays on the launching
	// session (or waits for the 5 s refresh throttle), so the panel keeps showing the old servers.
	it("shows the servers of the agent picked in the dropdown at once", async () => {
		const { send } = await boot();
		const first = await send({ kind: "snapshot" });
		expect(first.result.servers.servers.map((s: { url: string }) => s.url)).toEqual(serversOf.A);

		const picked = await send({ kind: "select", tabId: 7, sessionId: "B" });
		expect(picked.result.selectedSessionId).toBe("B");
		expect(picked.result.servers.servers.map((s: { url: string }) => s.url)).toEqual(serversOf.B);
		expect(calls.filter((c) => c.action === "list_dev_servers").map((c) => c.sessionId)).toEqual(["A", "B"]);
	});

	// Catches: open_server validating against another session's list, so a server of the
	// launching agent can still be opened after the pick moved to a different agent.
	it("refuses to open a server that is not in the selected agent's list", async () => {
		const { send, tabsUpdate } = await boot();
		await send({ kind: "snapshot" });
		await send({ kind: "select", tabId: 7, sessionId: "B" });
		const stale = await send({ kind: "open_server", url: serversOf.A[0] });
		expect(stale.ok).toBe(false);
		expect(tabsUpdate).not.toHaveBeenCalled();
		const fresh = await send({ kind: "open_server", url: serversOf.B[0] });
		expect(fresh.ok).toBe(true);
		expect(tabsUpdate).toHaveBeenCalledWith(7, { url: serversOf.B[0] });
	});
});
