import { beforeEach, describe, expect, it } from "vitest";
import { copyConnectUrl } from "../../../components/SettingsPanel/tabs/services/RemoteAccessPanel";
import { mockInvoke } from "../../mocks/tauri";

describe("copyConnectUrl", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
	});

	it("returns true when the clipboard write succeeds", async () => {
		mockInvoke.mockResolvedValue(undefined);

		await expect(copyConnectUrl("https://example.com/connect")).resolves.toBe(true);
		expect(mockInvoke).toHaveBeenCalledWith("write_clipboard_text", {
			text: "https://example.com/connect",
		});
	});

	it("returns false without throwing when the clipboard write is denied", async () => {
		mockInvoke.mockRejectedValue(new DOMException("Write permission denied.", "NotAllowedError"));

		await expect(copyConnectUrl("https://example.com/connect")).resolves.toBe(false);
	});
});
