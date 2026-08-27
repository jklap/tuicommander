import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";

const { mockWriteClipboard, mockAppLoggerError } = vi.hoisted(() => ({
	mockWriteClipboard: vi.fn<() => Promise<void>>(() => Promise.resolve()),
	mockAppLoggerError: vi.fn(),
}));

vi.mock("../../utils/clipboard", () => ({ writeClipboard: mockWriteClipboard }));

vi.mock("../../stores/appLogger", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../stores/appLogger")>();
	return { appLogger: { ...actual.appLogger, error: mockAppLoggerError } };
});

import { GeneratorsModal } from "../../components/GeneratorsModal/GeneratorsModal";

describe("GeneratorsModal", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.useFakeTimers();
		mockInvoke.mockResolvedValue({ value: "generated-value" });
	});

	afterEach(() => {
		cleanup();
		vi.useRealTimers();
		vi.restoreAllMocks();
	});

	it("shows 'Copied!' after a successful clipboard write", async () => {
		mockWriteClipboard.mockResolvedValueOnce(undefined);
		const { container } = render(() => <GeneratorsModal onClose={vi.fn()} />);
		await vi.advanceTimersByTimeAsync(0);
		expect((container.querySelector(".output") as HTMLTextAreaElement)?.value).toBe("generated-value");

		const copyBtn = container.querySelector(".copyBtn") as HTMLButtonElement;
		fireEvent.click(copyBtn);
		await vi.advanceTimersByTimeAsync(0);

		expect(copyBtn.textContent).toContain("Copied!");
	});

	it("does not show 'Copied!' and logs an error when the clipboard write fails", async () => {
		const err = new DOMException("Write permission denied.", "NotAllowedError");
		mockWriteClipboard.mockRejectedValueOnce(err);
		const { container } = render(() => <GeneratorsModal onClose={vi.fn()} />);
		await vi.advanceTimersByTimeAsync(0);
		expect((container.querySelector(".output") as HTMLTextAreaElement)?.value).toBe("generated-value");

		const copyBtn = container.querySelector(".copyBtn") as HTMLButtonElement;
		fireEvent.click(copyBtn);
		await vi.advanceTimersByTimeAsync(0);

		expect(mockAppLoggerError).toHaveBeenCalledWith("app", "Failed to copy generated value", err);
		expect(copyBtn.textContent).not.toContain("Copied!");
	});
});
