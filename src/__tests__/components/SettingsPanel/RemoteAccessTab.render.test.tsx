import { render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

// `RemoteAccessTab` fetches `get_local_ips` from a `createResource` on mount
// whose result nothing in this test's rendered tree ever reads — same leak
// shape documented in `DeepLinkSection.test.tsx` and previously guarded for
// this exact code when it lived in `ServicesTab.tsx` (see
// `ServicesTab.fileAccess.test.tsx`'s git history). Intercept only this one
// command — everything else keeps going through the real `rpc()`.
vi.mock("../../../transport", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../../transport")>();
	return {
		...actual,
		rpc: (async (command: string, ...rest: unknown[]) => {
			if (command === "get_local_ips") return [];
			// biome-ignore lint/suspicious/noExplicitAny: forwarding to the real overloaded rpc()
			return (actual.rpc as any)(command, ...rest);
		}) as typeof actual.rpc,
	};
});

import { RemoteAccessTab } from "../../../components/SettingsPanel/tabs/RemoteAccessTab";

/** `RemoteAccessTab`'s onMount fires several fire-and-forget rpc calls (status
 *  poll, load_config, tailscale status) that stay unresolved past the
 *  synchronous test body — flush them so vitest's leak detector doesn't flag
 *  them as dangling. */
async function flushMicrotasks(): Promise<void> {
	await new Promise<void>((resolve) => setImmediate(resolve));
}

describe("RemoteAccessTab — placement regression guard", () => {
	afterEach(async () => {
		vi.clearAllTimers();
		await flushMicrotasks();
	});

	it("renders every section moved out of Services & MCP", async () => {
		const { getByText, unmount } = render(() => <RemoteAccessTab />);
		await flushMicrotasks();

		expect(getByText("HTTP API Server")).toBeTruthy();
		expect(getByText("Remote Access")).toBeTruthy();
		expect(getByText("Cloud Relay")).toBeTruthy();
		expect(getByText("Enable remote access")).toBeTruthy();
		unmount();
	});
});
