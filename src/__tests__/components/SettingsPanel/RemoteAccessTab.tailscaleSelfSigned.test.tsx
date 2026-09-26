import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

// Fixture box mutated per-test — `rpc()` below reads it live on every call, so
// a test can move the fixture mid-test (e.g. before clicking "Recheck").
const rpcBox = vi.hoisted(() => ({
	tailscale: { state: "NotInstalled" } as unknown,
	selfSigned: null as unknown,
	raEnabled: true,
	recheckCalls: 0,
	regenerateCalls: 0,
}));

// Same leak shape as `RemoteAccessTab.render.test.tsx`/`RemoteAccessTab.fileAccess.test.tsx`
// (an unread `get_local_ips` createResource) — intercept only what this file needs and let
// everything else fall through to the real `rpc()`.
vi.mock("../../../transport", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../../transport")>();
	return {
		...actual,
		rpc: (async (command: string, ...rest: unknown[]) => {
			if (command === "get_local_ips") return [];
			if (command === "get_tailscale_status") return rpcBox.tailscale;
			if (command === "recheck_tailscale_status") {
				rpcBox.recheckCalls++;
				return rpcBox.tailscale;
			}
			if (command === "get_self_signed_cert_status") return rpcBox.selfSigned;
			if (command === "regenerate_self_signed_cert") {
				rpcBox.regenerateCalls++;
				return undefined;
			}
			if (command === "load_config") {
				return {
					services: {
						server: { enabled: rpcBox.raEnabled, port: 9876, ipv6_enabled: false },
						auth: {
							username: "admin",
							password_hash: "",
							session_token_duration_secs: 86400,
							lan_auth_bypass: false,
						},
						relay: { enabled: false, url: "", token: "", session_id: "" },
					},
				};
			}
			// biome-ignore lint/suspicious/noExplicitAny: forwarding to the real overloaded rpc()
			return (actual.rpc as any)(command, ...rest);
		}) as typeof actual.rpc,
	};
});

import { RemoteAccessTab } from "../../../components/SettingsPanel/tabs/RemoteAccessTab";

async function flushMicrotasks(): Promise<void> {
	await new Promise<void>((resolve) => setImmediate(resolve));
}

/** The `.mcpStatusDot` inside the `.group` immediately following a section's
 *  `<h3>` — scoped this way so a test never depends on which numbered dot in
 *  the whole tab (HTTP API Server / Cloud Relay have their own) is which. */
function statusDot(getByText: (text: string) => HTMLElement, heading: string): Element | null {
	const group = getByText(heading).nextElementSibling;
	return group?.querySelector(".mcpStatusDot") ?? null;
}

describe("RemoteAccessTab — Tailscale HTTPS", () => {
	beforeEach(() => {
		rpcBox.tailscale = { state: "NotInstalled" };
		rpcBox.selfSigned = null;
		rpcBox.raEnabled = true;
		rpcBox.recheckCalls = 0;
		rpcBox.regenerateCalls = 0;
	});

	afterEach(async () => {
		vi.clearAllTimers();
		await flushMicrotasks();
	});

	it("shows 'Not installed' with an unlit dot when Tailscale isn't installed", async () => {
		const { getByText, unmount } = render(() => <RemoteAccessTab />);
		await flushMicrotasks();

		expect(getByText("Tailscale HTTPS")).toBeTruthy();
		expect(getByText("Not installed")).toBeTruthy();
		expect(statusDot(getByText, "Tailscale HTTPS")?.className).not.toMatch(/\brunning\b/);
		unmount();
	});

	it("shows 'Not running' when Tailscale is installed but stopped", async () => {
		rpcBox.tailscale = { state: "NotRunning" };
		const { getByText, unmount } = render(() => <RemoteAccessTab />);
		await flushMicrotasks();

		expect(getByText("Not running")).toBeTruthy();
		unmount();
	});

	it("shows the HTTPS-not-enabled hint and an unlit dot when Tailscale is up but HTTPS isn't", async () => {
		rpcBox.tailscale = { state: "Running", fqdn: "box.tailnet.ts.net", https_enabled: false };
		const { getByText, unmount } = render(() => <RemoteAccessTab />);
		await flushMicrotasks();

		expect(getByText("Running (HTTPS not enabled)")).toBeTruthy();
		expect(getByText(/Enable HTTPS certificates/)).toBeTruthy();
		expect(statusDot(getByText, "Tailscale HTTPS")?.className).not.toMatch(/\brunning\b/);
		unmount();
	});

	it("shows the active fqdn and a lit dot once Tailscale HTTPS is active", async () => {
		rpcBox.tailscale = { state: "Running", fqdn: "box.tailnet.ts.net", https_enabled: true };
		const { getByText, queryByText, unmount } = render(() => <RemoteAccessTab />);
		await flushMicrotasks();

		expect(getByText("HTTPS active (box.tailnet.ts.net)")).toBeTruthy();
		expect(queryByText(/Enable HTTPS certificates/)).toBeNull();
		expect(statusDot(getByText, "Tailscale HTTPS")?.className).toMatch(/\brunning\b/);
		unmount();
	});

	it("Recheck re-fetches Tailscale status and re-renders it", async () => {
		rpcBox.tailscale = { state: "NotRunning" };
		const { getByText, unmount } = render(() => <RemoteAccessTab />);
		await flushMicrotasks();

		rpcBox.tailscale = { state: "Running", fqdn: "box.tailnet.ts.net", https_enabled: true };
		fireEvent.click(getByText("Recheck"));
		await flushMicrotasks();

		expect(rpcBox.recheckCalls).toBe(1);
		expect(getByText("HTTPS active (box.tailnet.ts.net)")).toBeTruthy();
		unmount();
	});
});

describe("RemoteAccessTab — Self-Signed HTTPS", () => {
	beforeEach(() => {
		rpcBox.tailscale = { state: "NotInstalled" };
		rpcBox.selfSigned = { active: false, generated: false, not_after_unix: null, fingerprint_sha256: null };
		rpcBox.raEnabled = true;
		rpcBox.recheckCalls = 0;
		rpcBox.regenerateCalls = 0;
	});

	afterEach(async () => {
		vi.clearAllTimers();
		await flushMicrotasks();
	});

	it("does not render when Tailscale HTTPS is already active", async () => {
		rpcBox.tailscale = { state: "Running", fqdn: "box.tailnet.ts.net", https_enabled: true };
		rpcBox.selfSigned = { active: true, generated: true, not_after_unix: null, fingerprint_sha256: "ab" };
		const { queryByText, unmount } = render(() => <RemoteAccessTab />);
		await flushMicrotasks();

		expect(queryByText("Self-Signed HTTPS")).toBeNull();
		unmount();
	});

	it("does not render when remote access is disabled", async () => {
		rpcBox.raEnabled = false;
		rpcBox.selfSigned = { active: true, generated: true, not_after_unix: null, fingerprint_sha256: "ab" };
		const { queryByText, unmount } = render(() => <RemoteAccessTab />);
		await flushMicrotasks();

		expect(queryByText("Self-Signed HTTPS")).toBeNull();
		unmount();
	});

	it("shows 'Not yet generated' with no Fingerprint row and an unlit dot when no cert exists yet", async () => {
		const { getByText, queryByText, unmount } = render(() => <RemoteAccessTab />);
		await flushMicrotasks();

		expect(getByText("Self-Signed HTTPS")).toBeTruthy();
		expect(getByText("Not yet generated")).toBeTruthy();
		expect(queryByText("Fingerprint")).toBeNull();
		expect(statusDot(getByText, "Self-Signed HTTPS")?.className).not.toMatch(/\brunning\b/);
		unmount();
	});

	it("shows 'Generated, not yet active' when generated but inactive", async () => {
		rpcBox.selfSigned = { active: false, generated: true, not_after_unix: null, fingerprint_sha256: "abcd1234" };
		const { getByText, unmount } = render(() => <RemoteAccessTab />);
		await flushMicrotasks();

		expect(getByText("Generated, not yet active")).toBeTruthy();
		unmount();
	});

	it("shows the active status with an expiry, a colon-separated fingerprint, and a lit dot", async () => {
		rpcBox.selfSigned = {
			active: true,
			generated: true,
			not_after_unix: Math.floor(new Date("2027-01-01T00:00:00Z").getTime() / 1000),
			fingerprint_sha256: "abcd1234",
		};
		const { getByText, unmount } = render(() => <RemoteAccessTab />);
		await flushMicrotasks();

		expect(getByText("Fingerprint")).toBeTruthy();
		expect(getByText("ab:cd:12:34")).toBeTruthy();
		expect(getByText(/^Active \(expires/)).toBeTruthy();
		expect(statusDot(getByText, "Self-Signed HTTPS")?.className).toMatch(/\brunning\b/);
		unmount();
	});

	it("Regenerate calls regenerate_self_signed_cert", async () => {
		rpcBox.selfSigned = { active: true, generated: true, not_after_unix: null, fingerprint_sha256: "ab" };
		const { getByText, unmount } = render(() => <RemoteAccessTab />);
		await flushMicrotasks();

		fireEvent.click(getByText("Regenerate"));
		await flushMicrotasks();

		expect(rpcBox.regenerateCalls).toBe(1);
		// The click schedules a real, non-`onCleanup`-tracked `setTimeout(refreshStatus, 500)`
		// (server restart is async server-side) — let it fire before unmounting, or vitest's
		// leak detector flags it as a dangling timer.
		await new Promise((resolve) => setTimeout(resolve, 550));
		await flushMicrotasks();
		unmount();
	});
});
