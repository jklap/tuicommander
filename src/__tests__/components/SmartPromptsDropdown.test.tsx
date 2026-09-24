/**
 * Story 706-8d98 — a Smart Prompt that cannot run explains why. The clickable
 * route to a Settings tab went away with the Providers tab (#784-0aec); the
 * reason now lives in the hover title= only, and the footer keeps its
 * "Manage Smart Prompts..." link.
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

// A prompt that cannot run shows its reason in the hover title= only. The
// clickable "route me to Settings" variant went with the Providers tab
// (#784-0aec), which was the only destination it ever had.
describe("SmartPromptsDropdown — disabled prompt", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		smartPromptsDropdownStore.open();
	});

	afterEach(() => {
		cleanup();
		smartPromptsDropdownStore.close();
	});

	it("keeps a disabled reason out of the rendered text", async () => {
		mockCanExecute.mockReturnValue({ ok: false, reason: "Agent is busy" });
		const onOpenSettings = vi.fn();
		const { container, queryByText } = render(() => <SmartPromptsDropdown onOpenSettings={onOpenSettings} />);
		// Drain the dropdown's own open-effect (rAF-scheduled search-input focus)
		// so it doesn't fire after the test tears down.
		await new Promise((r) => setImmediate(r));

		expect(queryByText("Agent is busy")).toBeNull();
		expect(container.querySelector("button")?.textContent).not.toContain("Agent is busy");
	});

	it("does not run a prompt that cannot execute", async () => {
		mockCanExecute.mockReturnValue({ ok: false, reason: "Agent is busy" });
		const { container } = render(() => <SmartPromptsDropdown onOpenSettings={vi.fn()} />);
		await new Promise((r) => setImmediate(r));

		const item = container.querySelector('[title="Agent is busy"]') as HTMLElement | null;
		expect(item).not.toBeNull();
		fireEvent.click(item as HTMLElement);
		expect(mockExecuteSmartPrompt).not.toHaveBeenCalled();
	});
});
