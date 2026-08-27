import { beforeEach, describe, expect, it, vi } from "vitest";

const { mockWriteClipboard, mockAppLoggerWarn } = vi.hoisted(() => ({
	mockWriteClipboard: vi.fn(),
	mockAppLoggerWarn: vi.fn(),
}));

vi.mock("../../utils/clipboard", () => ({ writeClipboard: mockWriteClipboard }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: mockAppLoggerWarn } }));

import { copyToClipboard } from "../../components/KnowledgeHistory/KnowledgeHistoryOverlay";

describe("KnowledgeHistoryOverlay copyToClipboard", () => {
	beforeEach(() => {
		mockWriteClipboard.mockReset();
		mockAppLoggerWarn.mockReset();
	});

	it("resolves without logging when the write succeeds", async () => {
		mockWriteClipboard.mockResolvedValue(undefined);

		await expect(copyToClipboard("some command")).resolves.toBeUndefined();

		expect(mockAppLoggerWarn).not.toHaveBeenCalled();
	});

	it("logs a warning instead of throwing when the write is denied", async () => {
		const err = new DOMException("Write permission denied.", "NotAllowedError");
		mockWriteClipboard.mockRejectedValue(err);

		await expect(copyToClipboard("some command")).resolves.toBeUndefined();

		expect(mockAppLoggerWarn).toHaveBeenCalledWith("ai-agent", expect.stringContaining("clipboard write failed"));
	});
});
