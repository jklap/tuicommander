import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const ptyMocks = vi.hoisted(() => ({
	sendCommand: vi.fn().mockResolvedValue(undefined),
}));

const terminalMocks = vi.hoisted(() => ({
	openComposeWithText: vi.fn(),
	isComposeOpen: vi.fn(() => false),
}));

const mockInvoke = vi.hoisted(() => vi.fn());

const smartPromptsMocks = vi.hoisted(() => ({
	executeSmartPrompt: vi.fn().mockResolvedValue({ ok: true }),
}));

const toastMocks = vi.hoisted(() => ({
	add: vi.fn(),
}));

vi.mock("../../invoke", () => ({ invoke: mockInvoke }));

vi.mock("../../stores/toasts", () => ({
	toastsStore: { add: (...args: unknown[]) => toastMocks.add(...args) },
}));

vi.mock("../../hooks/usePty", () => ({
	usePty: () => ({
		sendCommand: ptyMocks.sendCommand,
	}),
}));

// Preserve the real `resolveInjectTarget`/`shouldSubmitInjectPrompt` pure
// functions (PromptDrawer imports them directly for inject-mode prompts) but
// replace the hook factory, since executing a real shell/headless/api prompt
// needs a much larger dependency graph (agentConfigsStore, providerRegistryStore,
// repositoriesStore, githubStore, IPC commands) that is out of scope for this
// dialog's own tests — those paths are covered by useSmartPrompts.test.ts.
vi.mock("../../hooks/useSmartPrompts", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../hooks/useSmartPrompts")>();
	return {
		...actual,
		useSmartPrompts: () => ({
			executeSmartPrompt: smartPromptsMocks.executeSmartPrompt,
			canExecute: vi.fn(() => ({ ok: true })),
			resolveAllVariables: vi.fn().mockResolvedValue({}),
		}),
	};
});

vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		getActive: () => ({
			id: "terminal-1",
			sessionId: "session-1",
			agentType: "codex",
			ref: { openComposeWithText: terminalMocks.openComposeWithText, isComposeOpen: terminalMocks.isComposeOpen },
		}),
	},
}));

import { PromptDrawer } from "../../components/PromptDrawer/PromptDrawer";
import { promptLibraryStore } from "../../stores/promptLibrary";

function buttonByText(container: HTMLElement, text: string): HTMLButtonElement {
	const button = Array.from(container.querySelectorAll("button")).find(
		(candidate) => candidate.textContent?.trim() === text,
	);
	expect(button, `button "${text}" not found`).toBeTruthy();
	return button as HTMLButtonElement;
}

function createPromptThroughEditor(
	container: HTMLElement,
	name: string,
	content: string,
	autoExecute: boolean,
): HTMLElement {
	fireEvent.click(buttonByText(container, "+ New Prompt"));

	const nameInput = container.querySelector('input[placeholder="My Prompt"]') as HTMLInputElement;
	const contentInput = container.querySelector(
		'textarea[placeholder="Enter your prompt text here..."]',
	) as HTMLTextAreaElement;
	expect(nameInput).toBeTruthy();
	expect(contentInput).toBeTruthy();
	fireEvent.input(nameInput, { target: { value: name } });
	fireEvent.input(contentInput, { target: { value: content } });

	if (autoExecute) {
		const checkbox = Array.from(container.querySelectorAll('input[type="checkbox"]')).find((candidate) =>
			candidate.closest("label")?.textContent?.includes("Send immediately"),
		) as HTMLInputElement | undefined;
		expect(checkbox).toBeTruthy();
		fireEvent.click(checkbox!);
	}

	fireEvent.click(buttonByText(container, "Save"));
	const row = Array.from(container.querySelectorAll(".promptItem")).find((candidate) =>
		candidate.textContent?.includes(name),
	);
	expect(row).toBeTruthy();
	return row as HTMLElement;
}

describe("PromptDrawer auto-execute", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.clearAllMocks();
		terminalMocks.isComposeOpen.mockReturnValue(false);
		smartPromptsMocks.executeSmartPrompt.mockReset().mockResolvedValue({ ok: true });
		mockInvoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
			if (command === "extract_prompt_variables") {
				const content = String(args?.content ?? "");
				return Array.from(content.matchAll(/\{([^{}]+)\}/g), (match) => match[1]);
			}
			if (command === "process_prompt_content") {
				const variables = (args?.variables ?? {}) as Record<string, string>;
				return String(args?.content ?? "").replace(/\{([^{}]+)\}/g, (_match, name: string) => variables[name] ?? "");
			}
			return undefined;
		});
		for (const prompt of promptLibraryStore.getAllPrompts()) {
			promptLibraryStore.deletePrompt(prompt.id);
		}
		promptLibraryStore._testCancelPendingSave();
		promptLibraryStore.setSelectedCategory("all");
		promptLibraryStore.openDrawer();
	});

	afterEach(() => {
		promptLibraryStore.closeDrawer();
		promptLibraryStore._testCancelPendingSave();
		vi.useRealTimers();
	});

	it("submits an editor-created prompt with autoExecute enabled exactly once", async () => {
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Run custom prompt", "Do the custom task", true);

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(250);

		expect(ptyMocks.sendCommand).toHaveBeenCalledOnce();
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("session-1", "Do the custom task", "codex", true);
		expect(terminalMocks.openComposeWithText).not.toHaveBeenCalled();
	});

	it("keeps an editor-created prompt editable when autoExecute is disabled and Compose is closed (adaptive target routes to terminal)", async () => {
		terminalMocks.isComposeOpen.mockReturnValue(false);
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Review custom prompt", "Review before sending", false);

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(250);

		expect(ptyMocks.sendCommand).toHaveBeenCalledOnce();
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("session-1", "Review before sending", "codex", false);
		expect(terminalMocks.openComposeWithText).not.toHaveBeenCalled();
	});

	it("keeps an editor-created prompt editable when autoExecute is disabled and Compose is already open (adaptive target routes to compose)", async () => {
		terminalMocks.isComposeOpen.mockReturnValue(true);
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Review custom prompt compose open", "Review in compose", false);

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(250);

		expect(terminalMocks.openComposeWithText).toHaveBeenCalledOnce();
		expect(terminalMocks.openComposeWithText).toHaveBeenCalledWith("Review in compose");
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();
	});

	it("turns a double-click into one explicit submission without also inserting", async () => {
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Double-click prompt", "Run on double-click", false);

		fireEvent.click(row, { detail: 1 });
		fireEvent.click(row, { detail: 2 });
		fireEvent.dblClick(row, { detail: 2 });
		await vi.advanceTimersByTimeAsync(250);

		expect(ptyMocks.sendCommand).toHaveBeenCalledOnce();
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("session-1", "Run on double-click", "codex", true);
		expect(terminalMocks.openComposeWithText).not.toHaveBeenCalled();
	});

	it("injects a slow double-click only once", async () => {
		// The 200ms de-dup timer is shorter than macOS's configurable double-click
		// interval (up to ~1s). When the timer wins the race the single-click
		// injection has already run, and the dblclick that follows must not inject
		// the prompt a second time — least of all submitting it.
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Slow double-click", "Do it once", false);

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(400); // the pending click fires first
		fireEvent.click(row, { detail: 2 });
		fireEvent.dblClick(row, { detail: 2 });
		await vi.advanceTimersByTimeAsync(250);

		// What actually holds the line is `doInject` closing the drawer: the row
		// unmounts, so the trailing dblclick reaches no handler at all. That is load
		// bearing, not incidental — this test fails the moment injection stops
		// closing the drawer.
		expect(document.body.contains(row)).toBe(false);
		const injections = ptyMocks.sendCommand.mock.calls.length + terminalMocks.openComposeWithText.mock.calls.length;
		expect(injections).toBe(1);
	});

	it("lets Insert and Run override a disabled autoExecute flag after variable entry", async () => {
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Variable prompt", "Handle {topic}", false);

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(250);
		const variableInput = container.querySelector('input[placeholder="topic"]') as HTMLInputElement;
		expect(variableInput).toBeTruthy();
		fireEvent.input(variableInput, { target: { value: "tests" } });
		fireEvent.click(buttonByText(container, "Insert & Run"));
		await vi.waitFor(() => expect(ptyMocks.sendCommand).toHaveBeenCalledOnce());

		expect(ptyMocks.sendCommand).toHaveBeenCalledOnce();
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("session-1", "Handle tests", "codex", true);
		expect(terminalMocks.openComposeWithText).not.toHaveBeenCalled();
	});

	it("lets Insert override an enabled autoExecute flag after variable entry, routing to terminal when Compose is closed", async () => {
		terminalMocks.isComposeOpen.mockReturnValue(false);
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Editable variable prompt", "Handle {topic}", true);

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(250);
		const variableInput = container.querySelector('input[placeholder="topic"]') as HTMLInputElement;
		fireEvent.input(variableInput, { target: { value: "tests" } });
		fireEvent.click(buttonByText(container, "Insert"));
		await vi.waitFor(() => expect(ptyMocks.sendCommand).toHaveBeenCalledOnce());

		expect(ptyMocks.sendCommand).toHaveBeenCalledOnce();
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("session-1", "Handle tests", "codex", false);
		expect(terminalMocks.openComposeWithText).not.toHaveBeenCalled();
	});

	it("lets Insert override an enabled autoExecute flag after variable entry, routing to compose when it's already open", async () => {
		terminalMocks.isComposeOpen.mockReturnValue(true);
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Editable variable prompt compose open", "Handle {topic}", true);

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(250);
		const variableInput = container.querySelector('input[placeholder="topic"]') as HTMLInputElement;
		fireEvent.input(variableInput, { target: { value: "tests" } });
		fireEvent.click(buttonByText(container, "Insert"));
		await vi.waitFor(() => expect(terminalMocks.openComposeWithText).toHaveBeenCalledOnce());

		expect(terminalMocks.openComposeWithText).toHaveBeenCalledOnce();
		expect(terminalMocks.openComposeWithText).toHaveBeenCalledWith("Handle tests");
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();
	});

	it("focuses the search input on open (autofocus is inert — the drawer mounts once at app startup)", async () => {
		const { container } = render(() => <PromptDrawer />);
		const searchInput = container.querySelector(
			'input[placeholder="Search prompts... (type to filter)"]',
		) as HTMLInputElement;
		expect(searchInput).toBeTruthy();

		await vi.advanceTimersByTimeAsync(0);

		expect(document.activeElement).toBe(searchInput);
	});

	it("routes a shell-mode prompt through executeSmartPrompt instead of injecting its text", async () => {
		const prompt = promptLibraryStore.createPrompt({
			name: "Shell prompt",
			content: "echo hi",
			category: "custom",
			isFavorite: false,
			executionMode: "shell",
			enabled: true,
		});
		const { container } = render(() => <PromptDrawer />);
		const row = Array.from(container.querySelectorAll(".promptItem")).find((c) =>
			c.textContent?.includes("Shell prompt"),
		) as HTMLElement;
		expect(row).toBeTruthy();

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(250);

		expect(smartPromptsMocks.executeSmartPrompt).toHaveBeenCalledOnce();
		expect(smartPromptsMocks.executeSmartPrompt).toHaveBeenCalledWith(expect.objectContaining({ id: prompt.id }));
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();
		expect(terminalMocks.openComposeWithText).not.toHaveBeenCalled();
	});

	it("routes a headless-mode prompt through executeSmartPrompt on a single click, double-click, and Enter alike", async () => {
		const prompt = promptLibraryStore.createPrompt({
			name: "Headless prompt",
			content: "do the thing",
			category: "custom",
			isFavorite: false,
			executionMode: "headless",
			enabled: true,
		});
		const { container } = render(() => <PromptDrawer />);
		const row = Array.from(container.querySelectorAll(".promptItem")).find((c) =>
			c.textContent?.includes("Headless prompt"),
		) as HTMLElement;

		fireEvent.dblClick(row, { detail: 2 });
		await vi.advanceTimersByTimeAsync(250);

		expect(smartPromptsMocks.executeSmartPrompt).toHaveBeenCalledWith(expect.objectContaining({ id: prompt.id }));
	});

	it("does not close the drawer when executeSmartPrompt fails, and does not fall back to inserting text", async () => {
		smartPromptsMocks.executeSmartPrompt.mockResolvedValue({ ok: false, reason: "boom" });
		promptLibraryStore.createPrompt({
			name: "Failing api prompt",
			content: "run it",
			category: "custom",
			isFavorite: false,
			executionMode: "api",
			enabled: true,
		});
		const { container } = render(() => <PromptDrawer />);
		const row = Array.from(container.querySelectorAll(".promptItem")).find((c) =>
			c.textContent?.includes("Failing api prompt"),
		) as HTMLElement;

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(250);

		expect(document.body.contains(row)).toBe(true);
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();
		expect(terminalMocks.openComposeWithText).not.toHaveBeenCalled();
		expect(toastMocks.add).toHaveBeenCalledWith('"Failing api prompt" failed', "boom", "error");
	});
});

describe("PromptDrawer — category cycling, footer, placement, and modal registration", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.clearAllMocks();
		terminalMocks.isComposeOpen.mockReturnValue(false);
		smartPromptsMocks.executeSmartPrompt.mockReset().mockResolvedValue({ ok: true });
		mockInvoke.mockResolvedValue(undefined);
		for (const prompt of promptLibraryStore.getAllPrompts()) {
			promptLibraryStore.deletePrompt(prompt.id);
		}
		promptLibraryStore._testCancelPendingSave();
		promptLibraryStore.setSelectedCategory("all");
		promptLibraryStore.openDrawer();
	});

	afterEach(() => {
		promptLibraryStore.closeDrawer();
		promptLibraryStore._testCancelPendingSave();
		vi.useRealTimers();
	});

	it("Tab cycles the category chips forward, wrapping at the end, without moving focus out of Search", async () => {
		const { container } = render(() => <PromptDrawer />);
		const searchInput = container.querySelector(
			'input[placeholder="Search prompts... (type to filter)"]',
		) as HTMLInputElement;
		await vi.advanceTimersByTimeAsync(0);
		expect(document.activeElement).toBe(searchInput);

		fireEvent.keyDown(document, { key: "Tab" });
		expect(promptLibraryStore.state.selectedCategory).toBe("custom");
		expect(document.activeElement).toBe(searchInput);

		fireEvent.keyDown(document, { key: "Tab" });
		expect(promptLibraryStore.state.selectedCategory).toBe("recent");

		fireEvent.keyDown(document, { key: "Tab" });
		expect(promptLibraryStore.state.selectedCategory).toBe("favorite");

		fireEvent.keyDown(document, { key: "Tab" });
		expect(promptLibraryStore.state.selectedCategory).toBe("all");
		expect(document.activeElement).toBe(searchInput);
	});

	it("Shift+Tab cycles the category chips backward, wrapping at the start", async () => {
		render(() => <PromptDrawer />);
		await vi.advanceTimersByTimeAsync(0);

		fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
		expect(promptLibraryStore.state.selectedCategory).toBe("favorite");

		fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
		expect(promptLibraryStore.state.selectedCategory).toBe("recent");
	});

	it("clicking a category chip refocuses the search input", async () => {
		const { container } = render(() => <PromptDrawer />);
		const searchInput = container.querySelector(
			'input[placeholder="Search prompts... (type to filter)"]',
		) as HTMLInputElement;
		await vi.advanceTimersByTimeAsync(0);
		searchInput.blur();

		const customChip = Array.from(container.querySelectorAll('[role="tab"]')).find(
			(el) => el.textContent === "Custom",
		) as HTMLButtonElement;
		fireEvent.click(customChip);

		expect(promptLibraryStore.state.selectedCategory).toBe("custom");
		expect(document.activeElement).toBe(searchInput);
	});

	it("renders the footer as key-hint chips, matching the Command Palette's footer structure", () => {
		const { container } = render(() => <PromptDrawer />);
		const kbds = Array.from(container.querySelectorAll("kbd")).map((k) => k.textContent);
		expect(kbds).toEqual(["↑↓", "↵", "⌘E", "⌘F", "esc", "⇥"]);
	});

	it("renders mapped placement labels, not raw enum values, and includes file-context", async () => {
		const { container } = render(() => <PromptDrawer />);
		fireEvent.click(buttonByText(container, "+ New Prompt"));

		const placementLabels = Array.from(container.querySelectorAll(".placementCheck span")).map((el) => el.textContent);
		expect(placementLabels).toContain("Toolbar menu");
		expect(placementLabels).toContain("File right-click menu");
		expect(placementLabels).not.toContain("toolbar");
		expect(placementLabels).not.toContain("file-context");
	});

	it("registers with the central modal stack so Escape does not leak past the drawer", async () => {
		render(() => <PromptDrawer />);
		await vi.advanceTimersByTimeAsync(0);
		expect(promptLibraryStore.state.drawerOpen).toBe(true);

		fireEvent.keyDown(document, { key: "Escape" });

		expect(promptLibraryStore.state.drawerOpen).toBe(false);
	});

	it("Escape while the Prompt Editor sub-view is open closes just the editor, not the whole drawer", async () => {
		// Regression: registerModal's capture-phase listener always wins the race
		// against this component's own bubble-phase handleKeydown, so Escape must
		// be fully handled by the registered callback itself (including backing
		// out of a nested sub-view one layer at a time) — not just close the
		// drawer outright the way a naive registration would.
		const { container } = render(() => <PromptDrawer />);
		fireEvent.click(buttonByText(container, "+ New Prompt"));
		expect(container.querySelector('input[placeholder="My Prompt"]')).toBeTruthy();

		fireEvent.keyDown(document, { key: "Escape" });

		expect(container.querySelector('input[placeholder="My Prompt"]')).toBeFalsy();
		expect(promptLibraryStore.state.drawerOpen).toBe(true);
	});

	it("Escape while the Variable Input dialog is open closes just the dialog, not the whole drawer", async () => {
		mockInvoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
			if (command === "extract_prompt_variables") return ["topic"];
			if (command === "process_prompt_content") return String(args?.content ?? "");
			return undefined;
		});
		promptLibraryStore.createPrompt({
			name: "Variable prompt",
			content: "Handle {topic}",
			category: "custom",
			isFavorite: false,
			enabled: true,
		});
		const { container } = render(() => <PromptDrawer />);
		const row = Array.from(container.querySelectorAll(".promptItem")).find((c) =>
			c.textContent?.includes("Variable prompt"),
		) as HTMLElement;
		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(250);
		expect(container.querySelector('input[placeholder="topic"]')).toBeTruthy();

		fireEvent.keyDown(document, { key: "Escape" });

		expect(container.querySelector('input[placeholder="topic"]')).toBeFalsy();
		expect(promptLibraryStore.state.drawerOpen).toBe(true);
	});

	it("Tab refocuses the search input even when a different element currently has focus", async () => {
		const { container } = render(() => <PromptDrawer />);
		await vi.advanceTimersByTimeAsync(0);
		const searchInput = container.querySelector(
			'input[placeholder="Search prompts... (type to filter)"]',
		) as HTMLInputElement;
		const newPromptButton = buttonByText(container, "+ New Prompt");
		newPromptButton.focus();
		expect(document.activeElement).toBe(newPromptButton);

		fireEvent.keyDown(document, { key: "Tab" });

		expect(promptLibraryStore.state.selectedCategory).toBe("custom");
		expect(document.activeElement).toBe(searchInput);
	});

	it("does not reset keyboard selection to the top when an unrelated store write (favorite toggle) leaves the filtered set unchanged", async () => {
		const first = promptLibraryStore.createPrompt({
			name: "Alpha prompt",
			content: "a",
			category: "custom",
			isFavorite: false,
			enabled: true,
		});
		promptLibraryStore.createPrompt({
			name: "Beta prompt",
			content: "b",
			category: "custom",
			isFavorite: false,
			enabled: true,
		});
		const { container } = render(() => <PromptDrawer />);
		await vi.advanceTimersByTimeAsync(0);

		fireEvent.keyDown(document, { key: "ArrowDown" });
		const selectedBefore = container.querySelector(".selected");
		expect(selectedBefore?.textContent).toContain("Beta prompt");

		promptLibraryStore.toggleFavorite(first.id);

		const selectedAfter = container.querySelector(".selected");
		expect(selectedAfter?.textContent).toContain("Beta prompt");
	});

	it("resets selection to the top when a search narrows to a different, same-size set of prompts", async () => {
		promptLibraryStore.createPrompt({
			name: "Zeta prompt",
			content: "zebra content",
			category: "custom",
			isFavorite: false,
			enabled: true,
		});
		promptLibraryStore.createPrompt({
			name: "Yankee prompt",
			content: "yankee content",
			category: "custom",
			isFavorite: false,
			enabled: true,
		});
		const { container } = render(() => <PromptDrawer />);
		await vi.advanceTimersByTimeAsync(0);

		fireEvent.keyDown(document, { key: "ArrowDown" });
		expect(container.querySelector(".selected")?.textContent).toContain("Yankee prompt");

		// Narrows from 2 results to 1 — a real list-identity change, not an
		// unrelated store write — must reset selection back to index 0.
		const searchInput = container.querySelector(
			'input[placeholder="Search prompts... (type to filter)"]',
		) as HTMLInputElement;
		fireEvent.input(searchInput, { target: { value: "zebra" } });

		const selected = container.querySelector(".selected");
		expect(selected?.textContent).toContain("Zeta prompt");
	});
});
