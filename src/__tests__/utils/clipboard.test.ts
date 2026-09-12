import { beforeEach, describe, expect, it, vi } from "vitest";
import { copyPathToClipboard } from "../../utils/clipboard";
import { mockInvoke } from "../mocks/tauri";

describe("copyPathToClipboard", () => {
	beforeEach(() => {
		mockInvoke.mockClear();
	});

	// In Tauri mode writeClipboard() routes through the native clipboard-manager
	// plugin (WKWebView rejects navigator.clipboard.writeText), so assert on the invoke.
	const copiedText = () =>
		mockInvoke.mock.calls.find(([cmd]) => cmd === "plugin:clipboard-manager|write_text")?.[1].text;

	it("compresses the user's home directory to ~", async () => {
		copyPathToClipboard("/Users/someone/Gits/project/src/main.rs");
		await vi.waitFor(() => expect(copiedText()).toBe("~/Gits/project/src/main.rs"));
	});

	it("leaves a path outside the home directory untouched", async () => {
		copyPathToClipboard("/etc/hosts");
		await vi.waitFor(() => expect(copiedText()).toBe("/etc/hosts"));
	});
});
