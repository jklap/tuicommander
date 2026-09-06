/**
 * Story 706-8d98 — the missing-provider reason in the Smart Prompts dropdown
 * must be visible (not just a hover title=) and its "Settings → Providers"
 * portion must be a clickable route to openSettings("providers").
 *
 * Reasons without a settingsTab (busy agent, no terminal, etc.) keep the
 * plain title= tooltip behavior — only a provider-routed reason gets the
 * inline clickable hint.
 */
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockCanExecute, mockExecuteSmartPrompt } = vi.hoisted(() => ({
	mockCanExecute: vi.fn(),
	mockExecuteSmartPrompt: vi.fn(),
}));

vi.mock("../../hooks/useSmartPrompts", () => ({
	useSmartPrompts: () => ({
		canExecute: mockCanExecute,
		executeSmartPrompt: mockExecuteSmartPrompt,
	}),
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

const PROMPT = {
	id: "p1",
	name: "Ask the LLM",
	content: "Do something",
	category: "custom",
	isFavorite: false,
	createdAt: 1,
	updatedAt: 1,
	executionMode: "api",
	tags: ["smart"],
};

vi.mock("../../stores/promptLibrary", () => ({
	promptLibraryStore: {
		getSmartByPlacement: vi.fn(() => [PROMPT]),
		markAsUsed: vi.fn(),
	},
}));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		getActive: vi.fn(() => ({ id: "t1", sessionId: "s1", agentType: "claude" })),
		isBusy: vi.fn(() => false),
	},
}));

import { SmartPromptsDropdown } from "../../components/SmartPromptsDropdown/SmartPromptsDropdown";
import { smartPromptsDropdownStore } from "../../stores/smartPromptsDropdown";

describe("SmartPromptsDropdown — missing-provider settings hint (#706-8d98)", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		smartPromptsDropdownStore.open();
	});

	afterEach(() => {
		cleanup();
		smartPromptsDropdownStore.close();
	});

	it("shows the reason text inline (not just as a title= tooltip) and clicking it routes to Settings → Providers", async () => {
		mockCanExecute.mockReturnValue({
			ok: false,
			reason: "Headless provider not configured — add a provider and assign the Headless slot in Settings → Providers",
			settingsTab: "providers",
		});
		const onOpenSettings = vi.fn();
		const { getByText } = render(() => <SmartPromptsDropdown onOpenSettings={onOpenSettings} />);
		// Drain the dropdown's own open-effect (rAF-scheduled search-input focus)
		// so it doesn't fire after the test tears down.
		await new Promise((r) => setImmediate(r));

		const hint = getByText(/Headless provider not configured/);
		// Must be a real clickable element, not text hidden inside a title= attribute.
		expect(hint.tagName).toBe("BUTTON");

		fireEvent.click(hint);
		expect(onOpenSettings).toHaveBeenCalledWith("providers");
		// Clicking the settings hint must not run the (still-unexecutable) prompt.
		expect(mockExecuteSmartPrompt).not.toHaveBeenCalled();
	});

	it("does not add a clickable hint for a disabled reason with no settingsTab", async () => {
		mockCanExecute.mockReturnValue({ ok: false, reason: "Agent is busy" });
		const onOpenSettings = vi.fn();
		const { container, queryByText } = render(() => <SmartPromptsDropdown onOpenSettings={onOpenSettings} />);
		await new Promise((r) => setImmediate(r));

		expect(queryByText("Agent is busy")).toBeNull();
		expect(container.querySelector("button")?.textContent).not.toContain("Agent is busy");
	});
});
