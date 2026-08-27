import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { routeHeadlessOutput } from "../../hooks/useSmartPrompts";
import { appLogger } from "../../stores/appLogger";
import type { SavedPrompt } from "../../stores/promptLibrary";
import { mockInvoke } from "../mocks/tauri";

function makePrompt(overrides: Partial<SavedPrompt> = {}): SavedPrompt {
	return {
		id: "p1",
		name: "Test Prompt",
		content: "",
		outputTarget: "clipboard",
		...overrides,
	} as SavedPrompt;
}

describe("routeHeadlessOutput", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		vi.spyOn(appLogger, "info").mockImplementation(() => {});
		vi.spyOn(appLogger, "error").mockImplementation(() => {});
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	describe("outputTarget: clipboard", () => {
		it("logs success only after the clipboard write resolves", async () => {
			mockInvoke.mockResolvedValue(undefined);
			const prompt = makePrompt({ name: "My Prompt" });

			routeHeadlessOutput(prompt, "output text");

			expect(appLogger.info).not.toHaveBeenCalled();
			await vi.waitFor(() => {
				expect(appLogger.info).toHaveBeenCalledWith("prompts", '"My Prompt" output copied to clipboard');
			});
			expect(appLogger.error).not.toHaveBeenCalled();
		});

		it("logs a failure and never claims success when the clipboard write is denied", async () => {
			const err = new DOMException("Write permission denied.", "NotAllowedError");
			mockInvoke.mockRejectedValue(err);
			const prompt = makePrompt({ name: "My Prompt" });

			routeHeadlessOutput(prompt, "output text");

			await vi.waitFor(() => {
				expect(appLogger.error).toHaveBeenCalledWith("prompts", "Failed to copy to clipboard", err);
			});
			expect(appLogger.info).not.toHaveBeenCalled();
		});

		it("does nothing when output is empty", () => {
			routeHeadlessOutput(makePrompt(), "");

			expect(mockInvoke).not.toHaveBeenCalled();
			expect(appLogger.info).not.toHaveBeenCalled();
		});
	});
});
