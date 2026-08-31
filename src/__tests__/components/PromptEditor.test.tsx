import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The "Edit Prompt" dialog (`PromptEditor`, a sub-component of `PromptDrawer`) has
// no direct test coverage anywhere else. These tests exercise it through the real
// `PromptDrawer` — the same way a user reaches it — plus, in the last block, a real
// `ComposePanel` mounted alongside it to reproduce the actual reported bug: opening
// Edit Prompt while a Compose panel is open used to be visually present but dead to
// clicks, because ComposePanel's focusout handler forced focus back to itself.

const terminalMocks = vi.hoisted(() => ({
	openComposeWithText: vi.fn(),
}));

vi.mock("../../hooks/usePty", () => ({
	usePty: () => ({ sendCommand: vi.fn().mockResolvedValue(undefined) }),
}));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		getActive: () => ({
			id: "terminal-1",
			sessionId: "session-1",
			agentType: "codex",
			ref: { openComposeWithText: terminalMocks.openComposeWithText },
		}),
	},
}));

import { ComposePanel } from "../../components/ComposePanel/ComposePanel";
import { PromptDrawer } from "../../components/PromptDrawer/PromptDrawer";
import { promptLibraryStore } from "../../stores/promptLibrary";

/** happy-dom implements requestAnimationFrame with setImmediate — flush one. */
function flushFrame(): Promise<void> {
	return new Promise((resolve) => setImmediate(resolve));
}

function editButtonFor(container: HTMLElement, name: string): HTMLButtonElement {
	const row = Array.from(container.querySelectorAll(".promptItem")).find((candidate) =>
		candidate.textContent?.includes(name),
	) as HTMLElement | undefined;
	expect(row, `prompt row "${name}" not found`).toBeTruthy();
	const button = row!.querySelector('button[title="Edit"]');
	expect(button, `Edit button for "${name}" not found`).toBeTruthy();
	return button as HTMLButtonElement;
}

function fieldByPlaceholder(container: HTMLElement, placeholder: string): HTMLInputElement | HTMLTextAreaElement {
	const el = container.querySelector(`[placeholder="${placeholder}"]`);
	expect(el, `field with placeholder "${placeholder}" not found`).toBeTruthy();
	return el as HTMLInputElement | HTMLTextAreaElement;
}

describe("PromptEditor (Edit/New Prompt dialog)", () => {
	beforeEach(() => {
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
	});

	it("opens as 'New Prompt' with empty fields when created fresh", () => {
		const { container, getByText } = render(() => <PromptDrawer />);
		fireEvent.click(getByText("+ New Prompt"));

		expect(getByText("New Prompt")).toBeTruthy();
		expect(fieldByPlaceholder(container, "My Prompt").value).toBe("");
		expect(fieldByPlaceholder(container, "Enter your prompt text here...").value).toBe("");
	});

	it("opens as 'Edit Prompt' with existing fields pre-filled", () => {
		promptLibraryStore.createPrompt({
			name: "Existing prompt",
			content: "Existing content",
			description: "A description",
			category: "custom",
			isFavorite: false,
			enabled: true,
			placement: [],
		});
		const { container, getByText } = render(() => <PromptDrawer />);
		fireEvent.click(editButtonFor(container, "Existing prompt"));

		expect(getByText("Edit Prompt")).toBeTruthy();
		expect(fieldByPlaceholder(container, "My Prompt").value).toBe("Existing prompt");
		expect(fieldByPlaceholder(container, "Enter your prompt text here...").value).toBe("Existing content");
	});

	it("focuses the Name field on open (autofocus is inert on a dynamically-inserted node)", async () => {
		const { container, getByText } = render(() => <PromptDrawer />);
		fireEvent.click(getByText("+ New Prompt"));

		const nameInput = fieldByPlaceholder(container, "My Prompt");
		await waitFor(() => expect(document.activeElement).toBe(nameInput));
	});

	it("edits Name and Content, and Save persists both", async () => {
		const created = promptLibraryStore.createPrompt({
			name: "Draft prompt",
			content: "Draft content",
			category: "custom",
			isFavorite: false,
			enabled: true,
			placement: [],
		});
		const { container, getByText } = render(() => <PromptDrawer />);
		fireEvent.click(editButtonFor(container, "Draft prompt"));

		fireEvent.input(fieldByPlaceholder(container, "My Prompt"), { target: { value: "Renamed prompt" } });
		fireEvent.input(fieldByPlaceholder(container, "Enter your prompt text here..."), {
			target: { value: "Updated content" },
		});
		fireEvent.click(getByText("Save"));

		await waitFor(() => {
			const saved = promptLibraryStore.getPrompt(created.id);
			expect(saved?.name).toBe("Renamed prompt");
			expect(saved?.content).toBe("Updated content");
		});
	});

	it("the Content textarea is sized for a full prompt body, not the 6-row default", () => {
		promptLibraryStore.createPrompt({
			name: "Sizing check",
			content: "x",
			category: "custom",
			isFavorite: false,
			enabled: true,
			placement: [],
		});
		const { container } = render(() => <PromptDrawer />);
		fireEvent.click(editButtonFor(container, "Sizing check"));

		const contentTextarea = fieldByPlaceholder(container, "Enter your prompt text here...") as HTMLTextAreaElement;
		expect(contentTextarea.getAttribute("rows")).toBe("10");
		expect(contentTextarea.className).toContain("editorContentTextarea");
	});

	describe("staying interactive while a Compose panel is open (regression)", () => {
		it("keeps focus in the Content textarea after clicking it, even though a Compose panel elsewhere is open", async () => {
			promptLibraryStore.createPrompt({
				name: "Compose-open edit",
				content: "Original",
				category: "custom",
				isFavorite: false,
				enabled: true,
				placement: [],
			});
			const [composeOpen] = createSignal(true);

			const { container } = render(() => (
				<>
					<ComposePanel
						isOpen={composeOpen}
						initialText={() => ""}
						onClose={vi.fn()}
						onSend={vi.fn()}
						onEnqueue={vi.fn()}
						canEnqueue={() => true}
						queuedCount={() => 0}
						onClearQueue={vi.fn()}
						onLoadQueue={vi.fn(async () => [])}
						onRemoveQueued={vi.fn()}
					/>
					<PromptDrawer />
				</>
			));

			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			// Compose's own initial-open effect unconditionally focuses itself two
			// animation frames after mount, independent of anything under test here
			// — settle it first so it can't fire mid-test and clobber our assertion.
			await flushFrame();
			await flushFrame();
			const composeContent = container.querySelector(".cm-content") as HTMLElement;
			composeContent.focus();
			expect(document.activeElement).toBe(composeContent);

			fireEvent.click(editButtonFor(container, "Compose-open edit"));
			// The dialog auto-focuses its own Name field on open (a real setTimeout(0),
			// scheduled independently of Compose's rAFs) — let that settle first so it
			// can't race the click below and steal focus back after the fact.
			const nameInput = fieldByPlaceholder(container, "My Prompt");
			await waitFor(() => expect(document.activeElement).toBe(nameInput));

			const contentTextarea = fieldByPlaceholder(container, "Enter your prompt text here...") as HTMLTextAreaElement;
			contentTextarea.focus();
			expect(document.activeElement).toBe(contentTextarea);

			// Regression: ComposePanel's focusout handler used to unconditionally
			// refocus itself one frame after ANY focus loss, regardless of where
			// focus actually went — silently pulling the caret back out of this
			// dialog's field on the very next frame.
			await flushFrame();
			await flushFrame();
			expect(document.activeElement).toBe(contentTextarea);

			fireEvent.input(contentTextarea, { target: { value: "Edited while Compose is open" } });
			expect(contentTextarea.value).toBe("Edited while Compose is open");
		});
	});
});
