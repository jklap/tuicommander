import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ActionEntry } from "../../actions/actionRegistry";
import { CommandPalette } from "../../components/CommandPalette/CommandPalette";
import { commandPaletteStore } from "../../stores/commandPalette";

// Real component + real commandPaletteStore (command mode = empty query). We
// drive the document-level capture-phase keydown handler and assert selection
// movement + Enter dispatch + Escape close — the keyboard-nav contract.

let seq = 0;
function action(id: string, label: string): ActionEntry {
	return { id, label, category: "test", execute: vi.fn() } as unknown as ActionEntry;
}

const flush = () => new Promise<void>((r) => queueMicrotask(r));

describe("CommandPalette keyboard navigation", () => {
	// Alphabetical baseSort with no matching recent actions ⇒ order is
	// Alpha, Beta, Gamma. IDs are unique per test so recentActions accumulated by
	// executeAction in earlier tests never reorders the list.
	let actions: ActionEntry[];

	beforeEach(() => {
		commandPaletteStore.close();
		commandPaletteStore.setQuery("");
		seq++;
		actions = [action(`a${seq}`, "Alpha"), action(`b${seq}`, "Beta"), action(`c${seq}`, "Gamma")];
	});
	afterEach(() => commandPaletteStore.close());

	async function openWith() {
		render(() => <CommandPalette actions={actions} />);
		commandPaletteStore.open();
		await flush(); // let the isOpen effect attach the keydown listener
	}

	it("Enter with no movement executes the first action and closes", async () => {
		await openWith();
		fireEvent.keyDown(document, { key: "Enter" });
		expect(actions[0].execute as ReturnType<typeof vi.fn>).toHaveBeenCalledTimes(1);
		expect(commandPaletteStore.state.isOpen).toBe(false);
	});

	it("ArrowDown moves the selection before Enter executes", async () => {
		await openWith();
		fireEvent.keyDown(document, { key: "ArrowDown" }); // 0 → 1 (Beta)
		fireEvent.keyDown(document, { key: "Enter" });
		expect(actions[1].execute as ReturnType<typeof vi.fn>).toHaveBeenCalledTimes(1);
		expect(actions[0].execute as ReturnType<typeof vi.fn>).not.toHaveBeenCalled();
	});

	it("ArrowDown clamps at the last item", async () => {
		await openWith();
		for (let i = 0; i < 5; i++) fireEvent.keyDown(document, { key: "ArrowDown" }); // clamp at 2 (Gamma)
		fireEvent.keyDown(document, { key: "Enter" });
		expect(actions[2].execute as ReturnType<typeof vi.fn>).toHaveBeenCalledTimes(1);
	});

	it("ArrowUp clamps at the first item", async () => {
		await openWith();
		fireEvent.keyDown(document, { key: "ArrowDown" }); // → 1
		fireEvent.keyDown(document, { key: "ArrowUp" }); // → 0
		fireEvent.keyDown(document, { key: "ArrowUp" }); // clamp at 0
		fireEvent.keyDown(document, { key: "Enter" });
		expect(actions[0].execute as ReturnType<typeof vi.fn>).toHaveBeenCalledTimes(1);
	});

	it("Escape closes the palette without executing anything", async () => {
		await openWith();
		fireEvent.keyDown(document, { key: "Escape" });
		expect(commandPaletteStore.state.isOpen).toBe(false);
		for (const a of actions) expect(a.execute as ReturnType<typeof vi.fn>).not.toHaveBeenCalled();
	});

	describe("scope chips", () => {
		let mixedActions: ActionEntry[];

		beforeEach(() => {
			seq++;
			mixedActions = [
				action(`alpha${seq}`, "Alpha action"),
				action(`beta${seq}`, "Beta action"),
				{ ...action(`prompt${seq}`, "Smart: Review diff"), category: "Smart Prompts" } as ActionEntry,
			];
		});

		async function openWithMixed() {
			const rendered = render(() => <CommandPalette actions={mixedActions} />);
			commandPaletteStore.open();
			await flush();
			return rendered;
		}

		function chipLabels(container: HTMLElement): string[] {
			return Array.from(container.querySelectorAll('[role="tab"]')).map((el) => el.textContent ?? "");
		}

		function activeChip(container: HTMLElement): string | undefined {
			return Array.from(container.querySelectorAll('[role="tab"]')).find(
				(el) => el.getAttribute("aria-selected") === "true",
			)?.textContent as string | undefined;
		}

		it("renders all six scope chips with 'All' active by default", async () => {
			const { container } = await openWithMixed();
			expect(chipLabels(container)).toEqual(["All", "Actions", "Prompts", "Files", "Content", "Terminals"]);
			expect(activeChip(container)).toBe("All");
		});

		it("Tab cycles forward through scopes without executing the highlighted action", async () => {
			const { container } = await openWithMixed();
			fireEvent.keyDown(document, { key: "Tab" });
			expect(commandPaletteStore.scope()).toBe("actions");
			expect(activeChip(container)).toBe("Actions");

			fireEvent.keyDown(document, { key: "Tab" });
			expect(commandPaletteStore.scope()).toBe("prompts");

			for (const a of mixedActions) expect(a.execute as ReturnType<typeof vi.fn>).not.toHaveBeenCalled();
		});

		it("Shift+Tab cycles backward and wraps", async () => {
			await openWithMixed();
			fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
			expect(commandPaletteStore.scope()).toBe("terminals");
		});

		it("clicking a chip selects that scope", async () => {
			const { container } = await openWithMixed();
			const promptsChip = Array.from(container.querySelectorAll('[role="tab"]')).find(
				(el) => el.textContent === "Prompts",
			)!;
			fireEvent.click(promptsChip);
			expect(commandPaletteStore.scope()).toBe("prompts");
		});

		it("'Prompts' scope shows only Smart Prompts actions", async () => {
			const { getByText, queryByText } = await openWithMixed();
			commandPaletteStore.setScope("prompts");
			await flush();

			expect(getByText("Smart: Review diff")).toBeTruthy();
			expect(queryByText("Alpha action")).toBeNull();
			expect(queryByText("Beta action")).toBeNull();
		});

		it("'Actions' scope excludes Smart Prompts actions", async () => {
			const { getByText, queryByText } = await openWithMixed();
			commandPaletteStore.setScope("actions");
			await flush();

			expect(getByText("Alpha action")).toBeTruthy();
			expect(getByText("Beta action")).toBeTruthy();
			expect(queryByText("Smart: Review diff")).toBeNull();
		});

		it("'All' scope shows every action, including Smart Prompts", async () => {
			const { getByText } = await openWithMixed();
			commandPaletteStore.setScope("actions");
			await flush();
			commandPaletteStore.setScope("all");
			await flush();

			expect(getByText("Alpha action")).toBeTruthy();
			expect(getByText("Smart: Review diff")).toBeTruthy();
		});
	});
});
