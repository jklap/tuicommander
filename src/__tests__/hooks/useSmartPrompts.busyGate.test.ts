/**
 * The inject busy gate must see the caller's explicit submit choice (Batch 39
 * review, PROBLEM 0fba1a938): a double-click or the variable dialog's Execute
 * submits even an `autoExecute: false` prompt, and used to skip the gate.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const { terminals } = vi.hoisted(() => ({
	terminals: { busy: true },
}));

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));

vi.mock("../../stores/settings", () => ({
	settingsStore: { isAcpConfigured: () => true },
}));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		getActive: () => ({
			id: "t1",
			sessionId: "s1",
			agentType: "claude",
			cwd: "/repo",
			ref: { isComposeOpen: () => false },
		}),
		isBusy: () => terminals.busy,
	},
}));

vi.mock("../../stores/repositories", () => ({
	repositoriesStore: { getActive: () => ({ path: "/repo" }), get: () => undefined },
}));

vi.mock("../../stores/github", () => ({
	githubStore: { getBranchPrData: () => null },
}));

vi.mock("../../stores/promptLibrary", () => ({
	promptLibraryStore: {
		markAsUsed: vi.fn(),
		processContent: async (prompt: { content: string }) => prompt.content,
	},
}));

vi.mock("../../stores/agentConfigs", () => ({
	agentConfigsStore: {
		getHeadlessAgent: () => null,
		getHeadlessTemplate: () => undefined,
		getRunConfigs: () => [],
	},
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../hooks/usePty", () => ({ usePty: () => ({ sendCommand: vi.fn() }) }));

import { useSmartPrompts } from "../../hooks/useSmartPrompts";
import type { SavedPrompt } from "../../stores/promptLibrary";

function reviewPrompt(): SavedPrompt {
	return {
		id: "p1",
		name: "Review",
		content: "review this",
		category: "custom",
		isFavorite: false,
		createdAt: 1,
		updatedAt: 1,
		executionMode: "inject",
		autoExecute: false,
		// Compose-preferred: a plain click only fills the compose box, never submits.
		injectTarget: "compose",
	} as SavedPrompt;
}

describe("canExecute busy gate with an explicit submit override", () => {
	beforeEach(() => {
		terminals.busy = true;
	});

	it("lets a review-only insertion through while the agent is busy", () => {
		expect(useSmartPrompts().canExecute(reviewPrompt()).ok).toBe(true);
	});

	it("blocks the same prompt when the caller forces a submit (double-click / Execute)", () => {
		const check = useSmartPrompts().canExecute(reviewPrompt(), true);
		expect(check).toEqual({ ok: false, reason: "Agent is busy" });
	});

	it("allows the forced submit once the agent is idle", () => {
		terminals.busy = false;
		expect(useSmartPrompts().canExecute(reviewPrompt(), true).ok).toBe(true);
	});
});
