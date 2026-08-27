import { beforeEach, describe, expect, it, vi } from "vitest";

const { mockWriteClipboard, mockToastAdd, mockAppLoggerError } = vi.hoisted(() => ({
	mockWriteClipboard: vi.fn(),
	mockToastAdd: vi.fn(),
	mockAppLoggerError: vi.fn(),
}));

vi.mock("../../../utils/clipboard", () => ({ writeClipboard: mockWriteClipboard }));
vi.mock("../../../stores/toasts", () => ({ toastsStore: { add: mockToastAdd } }));
vi.mock("../../../stores/appLogger", () => ({ appLogger: { error: mockAppLoggerError } }));

import { handleOsc52ClipboardStore } from "../osc52Clipboard";

describe("handleOsc52ClipboardStore", () => {
	beforeEach(() => {
		mockWriteClipboard.mockReset();
		mockToastAdd.mockReset();
		mockAppLoggerError.mockReset();
	});

	it("toasts success only after the clipboard write actually resolves", async () => {
		mockWriteClipboard.mockResolvedValue(undefined);

		handleOsc52ClipboardStore("copied text", "my-terminal");

		expect(mockToastAdd).not.toHaveBeenCalled();
		await vi.waitFor(() => {
			expect(mockToastAdd).toHaveBeenCalledWith("Clipboard updated", "by my-terminal", "info");
		});
		expect(mockAppLoggerError).not.toHaveBeenCalled();
	});

	it("does not show a success toast when the clipboard write is denied", async () => {
		const err = new DOMException("Write permission denied.", "NotAllowedError");
		mockWriteClipboard.mockRejectedValue(err);

		handleOsc52ClipboardStore("copied text", "my-terminal");

		await vi.waitFor(() => {
			expect(mockAppLoggerError).toHaveBeenCalledWith("terminal", "Failed to write OSC 52 clipboard update", err);
		});
		expect(mockToastAdd).not.toHaveBeenCalled();
	});
});
