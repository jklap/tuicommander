import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const smartPromptsMocks = vi.hoisted(() => ({
	executeSmartPrompt: vi.fn().mockResolvedValue({ ok: true }),
	canExecute: vi.fn(() => ({ ok: true })),
}));

// Replaces the real useSmartPrompts hook (executing a real shell/headless/api
// prompt needs a much larger dependency graph — see useSmartPrompts.test.ts
// for that coverage). This test is only about what SmartButtonStrip passes
// INTO executeSmartPrompt, not what executeSmartPrompt does with it.
vi.mock("../../hooks/useSmartPrompts", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../hooks/useSmartPrompts")>();
	return {
		...actual,
		useSmartPrompts: () => ({
			executeSmartPrompt: smartPromptsMocks.executeSmartPrompt,
			canExecute: smartPromptsMocks.canExecute,
			resolveAllVariables: vi.fn().mockResolvedValue({}),
		}),
	};
});

import { SmartButtonStrip } from "../../components/SmartButtonStrip/SmartButtonStrip";
import type { SavedPrompt } from "../../stores/promptLibrary";
import { promptLibraryStore } from "../../stores/promptLibrary";

function makePrompt(overrides: Partial<SavedPrompt> = {}): SavedPrompt {
	return {
		id: "smart-commit",
		name: "Commit",
		content: "Do something",
		category: "custom",
		isFavorite: false,
		createdAt: 1_000_000,
		updatedAt: 1_000_000,
		executionMode: "inject",
		...overrides,
	};
}

describe("SmartButtonStrip — targets its own repoPath", () => {
	// Regression coverage for the same caller-side bug fixed in
	// GitPanel/ChangesTab.tsx: every consumer of this component (GitHubPanel,
	// PrSection, PrDetailPopover, BranchesTab, ChangesTab) is bound to a
	// specific repo via its own `repoPath` prop, independent of which repo's
	// terminal happens to be focused. Without forwarding it, executeSmartPrompt
	// would fall back to resolving against the active terminal's tree instead.
	const REPO_PATH = "/repo/for/this/strip";

	beforeEach(() => {
		vi.spyOn(promptLibraryStore, "getSmartByPlacement").mockReturnValue([makePrompt()]);
		vi.spyOn(promptLibraryStore, "markAsUsed").mockImplementation(() => {});
		(promptLibraryStore.state as unknown as { recentIds: string[] }).recentIds = [];
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it("passes its own repoPath as executeSmartPrompt's targetPath when the main button is clicked", async () => {
		const { findByRole } = render(() => <SmartButtonStrip placement="git-changes" repoPath={REPO_PATH} />);

		const button = await findByRole("button", { name: /commit/i });
		button.click();
		await Promise.resolve();

		expect(smartPromptsMocks.executeSmartPrompt).toHaveBeenCalledWith(
			expect.objectContaining({ id: "smart-commit" }),
			undefined,
			REPO_PATH,
		);
	});

	it("passes its own repoPath when a dropdown menu item is clicked", async () => {
		vi.spyOn(promptLibraryStore, "getSmartByPlacement").mockReturnValue([
			makePrompt({ id: "smart-commit", name: "Commit" }),
			makePrompt({ id: "smart-amend", name: "Amend" }),
		]);

		const { findByTitle, findByRole } = render(() => <SmartButtonStrip placement="git-changes" repoPath={REPO_PATH} />);

		const arrow = await findByTitle("More actions");
		arrow.click();

		const amendItem = await findByRole("button", { name: /amend/i });
		amendItem.click();
		await Promise.resolve();

		expect(smartPromptsMocks.executeSmartPrompt).toHaveBeenCalledWith(
			expect.objectContaining({ id: "smart-amend" }),
			undefined,
			REPO_PATH,
		);
	});

	it("passes contextVariables through as manualVariables alongside repoPath", async () => {
		const { findByRole } = render(() => (
			<SmartButtonStrip placement="pr-popover" repoPath={REPO_PATH} contextVariables={() => ({ issue_number: "42" })} />
		));

		const button = await findByRole("button", { name: /commit/i });
		button.click();
		await Promise.resolve();

		expect(smartPromptsMocks.executeSmartPrompt).toHaveBeenCalledWith(
			expect.objectContaining({ id: "smart-commit" }),
			{ issue_number: "42" },
			REPO_PATH,
		);
	});
});
