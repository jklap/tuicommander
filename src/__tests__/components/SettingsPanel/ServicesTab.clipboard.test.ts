import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { copyConnectUrl, copyMcpSnippet } from "../../../components/SettingsPanel/tabs/ServicesTab";
import { appLogger } from "../../../stores/appLogger";
import { mockInvoke } from "../../mocks/tauri";

describe("copyConnectUrl", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
	});

	it("returns true when the clipboard write succeeds", async () => {
		mockInvoke.mockResolvedValue(undefined);

		await expect(copyConnectUrl("https://example.com/connect")).resolves.toBe(true);
		expect(mockInvoke).toHaveBeenCalledWith("plugin:clipboard-manager|write_text", {
			text: "https://example.com/connect",
			label: undefined,
		});
	});

	it("returns false without throwing when the clipboard write is denied", async () => {
		mockInvoke.mockRejectedValue(new DOMException("Write permission denied.", "NotAllowedError"));

		await expect(copyConnectUrl("https://example.com/connect")).resolves.toBe(false);
	});
});

describe("copyMcpSnippet", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		vi.spyOn(appLogger, "warn").mockImplementation(() => {});
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it("returns true and does not log when the clipboard write succeeds", async () => {
		mockInvoke.mockResolvedValue(undefined);

		await expect(copyMcpSnippet('{"mcpServers": {}}')).resolves.toBe(true);
		expect(appLogger.warn).not.toHaveBeenCalled();
	});

	it("returns false and logs a warning when the clipboard write is denied", async () => {
		const err = new DOMException("Write permission denied.", "NotAllowedError");
		mockInvoke.mockRejectedValue(err);

		await expect(copyMcpSnippet('{"mcpServers": {}}')).resolves.toBe(false);
		expect(appLogger.warn).toHaveBeenCalledWith("settings", "Clipboard write failed", { error: String(err) });
	});
});
