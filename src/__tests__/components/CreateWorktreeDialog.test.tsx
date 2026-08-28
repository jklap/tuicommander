import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CreateWorktreeDialog } from "../../components/CreateWorktreeDialog/CreateWorktreeDialog";
import { __resetModalStackForTest } from "../../stores/modalStack";

const defaultProps = {
	visible: true,
	suggestedName: "bold-nexus-042",
	existingBranches: ["main", "develop", "feature/auth", "fix/login-bug"],
	worktreeBranches: ["main"],
	worktreesDir: "/repos/myproject/.worktrees",
	onClose: () => {},
	onCreate: () => {},
	onGenerateName: vi.fn().mockResolvedValue("cool-ripley-007"),
};

const BASE_REFS = [
	{ name: "main", kind: "local", is_default: true },
	{ name: "develop", kind: "local", is_default: false },
	{ name: "origin/feature/x", kind: "remote", is_default: false },
];

describe("CreateWorktreeDialog", () => {
	beforeEach(() => {
		__resetModalStackForTest();
	});

	afterEach(() => {
		__resetModalStackForTest();
	});

	it("renders nothing when not visible", () => {
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} visible={false} />);
		expect(container.querySelector(".popover")).toBeNull();
	});

	it("renders dialog with input and branch list when visible", () => {
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
		expect(container.querySelector(".popover")).not.toBeNull();
		expect(container.querySelector("h4")!.textContent).toBe("New Worktree");

		const input = container.querySelector("input[type='text']") as HTMLInputElement;
		expect(input).not.toBeNull();
	});

	it("starts with empty input and shows all non-worktree branches", () => {
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
		const input = container.querySelector("input[type='text']") as HTMLInputElement;
		expect(input.value).toBe("");

		// Branch list should show all branches
		const items = container.querySelectorAll("[class*='branchItem']");
		expect(items.length).toBe(4); // main, develop, feature/auth, fix/login-bug
	});

	it("shows worktree branches as disabled with suffix", () => {
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
		// "main" has a worktree — should have disabled styling and "(has worktree)" text
		const items = container.querySelectorAll("[class*='branchItem']");
		const mainItem = Array.from(items).find((el) => el.textContent?.includes("main"));
		expect(mainItem).toBeDefined();
		expect(mainItem!.textContent).toContain("has worktree");
		expect(mainItem!.classList.toString()).toContain("disabled");
	});

	it("filters branch list as user types", () => {
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
		const input = container.querySelector("input[type='text']") as HTMLInputElement;

		fireEvent.input(input, { target: { value: "feat" } });

		const items = container.querySelectorAll("[class*='branchItem']");
		// Only "feature/auth" should match
		expect(items.length).toBe(1);
		expect(items[0].textContent).toContain("feature/auth");
	});

	it("shows 'existing branch' status when input matches an existing branch", () => {
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
		const input = container.querySelector("input[type='text']") as HTMLInputElement;

		fireEvent.input(input, { target: { value: "develop" } });

		const status = container.querySelector("[class*='statusLine']");
		expect(status).not.toBeNull();
		expect(status!.textContent).toContain("existing branch");
	});

	it("shows 'new branch' status when input doesn't match any branch", () => {
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
		const input = container.querySelector("input[type='text']") as HTMLInputElement;

		fireEvent.input(input, { target: { value: "feature/new-thing" } });

		const status = container.querySelector("[class*='statusLine']");
		expect(status).not.toBeNull();
		expect(status!.textContent).toContain("new branch");
	});

	it("shows path preview with sanitized branch name", () => {
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
		const input = container.querySelector("input[type='text']") as HTMLInputElement;

		fireEvent.input(input, { target: { value: "feature/auth" } });

		const path = container.querySelector("[class*='pathPreview']");
		expect(path).not.toBeNull();
		// Slashes in branch names should be replaced in the directory name
		expect(path!.textContent).toContain("/repos/myproject/.worktrees/");
		expect(path!.textContent).toContain("feature");
	});

	it("clicking a non-disabled branch populates input", () => {
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
		const items = container.querySelectorAll("[class*='branchItem']");
		const developItem = Array.from(items).find(
			(el) => el.textContent?.includes("develop") && !el.textContent?.includes("has worktree"),
		);
		expect(developItem).toBeDefined();

		fireEvent.click(developItem!);

		const input = container.querySelector("input[type='text']") as HTMLInputElement;
		expect(input.value).toBe("develop");
	});

	it("clicking a disabled (worktree) branch does NOT populate input", () => {
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
		const items = container.querySelectorAll("[class*='branchItem']");
		const mainItem = Array.from(items).find((el) => el.textContent?.includes("has worktree"));
		expect(mainItem).toBeDefined();

		fireEvent.click(mainItem!);

		const input = container.querySelector("input[type='text']") as HTMLInputElement;
		expect(input.value).toBe(""); // should remain empty
	});

	it("calls onCreate with createBranch=false for existing branch", () => {
		const handleCreate = vi.fn();
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />);
		const input = container.querySelector("input[type='text']") as HTMLInputElement;

		fireEvent.input(input, { target: { value: "develop" } });

		const createBtn = container.querySelector(".primaryBtn")!;
		fireEvent.click(createBtn);

		expect(handleCreate).toHaveBeenCalledWith({
			branchName: "develop",
			createBranch: false,
			baseRef: "",
		});
	});

	it("calls onCreate with createBranch=true for new branch", () => {
		const handleCreate = vi.fn();
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />);
		const input = container.querySelector("input[type='text']") as HTMLInputElement;

		fireEvent.input(input, { target: { value: "feature/new-thing" } });

		const createBtn = container.querySelector(".primaryBtn")!;
		fireEvent.click(createBtn);

		expect(handleCreate).toHaveBeenCalledWith({
			branchName: "feature/new-thing",
			createBranch: true,
			baseRef: "",
		});
	});

	it("disables Create button when input is empty", () => {
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
		const createBtn = container.querySelector(".primaryBtn") as HTMLButtonElement;
		expect(createBtn.disabled).toBe(true);
	});

	it("shows validation error for invalid branch name on create", () => {
		const handleCreate = vi.fn();
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />);
		const input = container.querySelector("input[type='text']") as HTMLInputElement;

		fireEvent.input(input, { target: { value: "bad name" } });

		const createBtn = container.querySelector(".primaryBtn")!;
		fireEvent.click(createBtn);

		// Should show error, not call onCreate
		const error = container.querySelector(".error");
		expect(error).not.toBeNull();
		expect(handleCreate).not.toHaveBeenCalled();
	});

	it("shows error when trying to create worktree for branch that already has one", () => {
		const handleCreate = vi.fn();
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />);
		const input = container.querySelector("input[type='text']") as HTMLInputElement;

		// "main" already has a worktree
		fireEvent.input(input, { target: { value: "main" } });

		const createBtn = container.querySelector(".primaryBtn")!;
		fireEvent.click(createBtn);

		const error = container.querySelector(".error");
		expect(error).not.toBeNull();
		expect(error!.textContent).toContain("already has a worktree");
		expect(handleCreate).not.toHaveBeenCalled();
	});

	it("calls onClose when cancel button is clicked", () => {
		const handleClose = vi.fn();
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} onClose={handleClose} />);
		const cancelBtn = container.querySelector(".cancelBtn")!;
		fireEvent.click(cancelBtn);
		expect(handleClose).toHaveBeenCalledOnce();
	});

	it("no status or path preview shown when input is empty", () => {
		const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
		const status = container.querySelector("[class*='statusLine']");
		const path = container.querySelector("[class*='pathPreview']");
		expect(status).toBeNull();
		expect(path).toBeNull();
	});

	describe("random name button", () => {
		it("renders a generate-name button", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
			const btn = container.querySelector("[class*='generateBtn']");
			expect(btn).not.toBeNull();
		});

		it("calls onGenerateName and populates input on click", async () => {
			const onGenerateName = vi.fn().mockResolvedValue("cool-ripley-007");
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} onGenerateName={onGenerateName} />);
			const btn = container.querySelector("[class*='generateBtn']")!;
			fireEvent.click(btn);

			expect(onGenerateName).toHaveBeenCalledOnce();

			await vi.waitFor(() => {
				const input = container.querySelector("input[type='text']") as HTMLInputElement;
				expect(input.value).toBe("cool-ripley-007");
			});
		});

		it("clears error when generating a name", async () => {
			const onGenerateName = vi.fn().mockResolvedValue("fresh-name-001");
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} onGenerateName={onGenerateName} />);

			// Trigger an error first
			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "bad name" } });
			const createBtn = container.querySelector(".primaryBtn")!;
			fireEvent.click(createBtn);
			expect(container.querySelector(".error")).not.toBeNull();

			// Click generate
			const btn = container.querySelector("[class*='generateBtn']")!;
			fireEvent.click(btn);

			await vi.waitFor(() => {
				expect(container.querySelector(".error")).toBeNull();
			});
		});
	});

	describe("base ref dropdown", () => {
		it("is hidden when there are fewer than 2 base refs", () => {
			const { container } = render(() => (
				<CreateWorktreeDialog {...defaultProps} baseRefs={[{ name: "main", kind: "local", is_default: true }]} />
			));
			expect(container.querySelector("[class*='dropdownTrigger']")).toBeNull();
		});

		it("is hidden once the typed name matches an existing branch", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			expect(container.querySelector("[class*='dropdownTrigger']")).not.toBeNull();

			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "develop" } });

			expect(container.querySelector("[class*='dropdownTrigger']")).toBeNull();
		});

		it("defaults the trigger to the first base ref and marks it as default", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			expect(trigger.textContent).toContain("main");

			fireEvent.click(trigger);
			const mainItem = Array.from(container.querySelectorAll("[class*='dropdownItem']")).find((el) =>
				el.textContent?.startsWith("main"),
			);
			expect(mainItem?.textContent).toContain("default");
			expect(mainItem?.classList.toString()).toContain("dropdownItemActive");
		});

		it("preselects defaultBaseRef instead of the backend default when provided", () => {
			const { container } = render(() => (
				<CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} defaultBaseRef="develop" />
			));
			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			expect(trigger.textContent).toContain("develop");
		});

		it("groups refs under Local and Remote section headers", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			fireEvent.click(trigger);

			const headers = Array.from(container.querySelectorAll("[class*='dropdownSectionHeader']")).map(
				(el) => el.textContent,
			);
			expect(headers).toEqual(["Local", "Remote"]);

			const remoteItem = Array.from(container.querySelectorAll("[class*='dropdownItem']")).find((el) =>
				el.textContent?.includes("origin/feature/x"),
			);
			expect(remoteItem).toBeDefined();
		});

		it("clicking an item closes the list and updates the trigger", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			fireEvent.click(trigger);

			const developItem = Array.from(container.querySelectorAll("[class*='dropdownItem']")).find(
				(el) => el.textContent?.trim() === "develop",
			)!;
			fireEvent.click(developItem);

			expect(container.querySelector("[class*='dropdownList']")).toBeNull();
			expect(trigger.textContent).toContain("develop");
		});

		it("passes the selected base ref through to onCreate for a new branch", () => {
			const handleCreate = vi.fn();
			const { container } = render(() => (
				<CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} onCreate={handleCreate} />
			));

			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			fireEvent.click(trigger);
			const developItem = Array.from(container.querySelectorAll("[class*='dropdownItem']")).find(
				(el) => el.textContent?.trim() === "develop",
			)!;
			fireEvent.click(developItem);

			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "feature/new-thing" } });

			const createBtn = container.querySelector(".primaryBtn")!;
			fireEvent.click(createBtn);

			expect(handleCreate).toHaveBeenCalledWith({
				branchName: "feature/new-thing",
				createBranch: true,
				baseRef: "develop",
			});
		});

		it("closes when clicking outside the dropdown", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			fireEvent.click(trigger);
			expect(container.querySelector("[class*='dropdownList']")).not.toBeNull();

			fireEvent.mouseDown(document.body);

			expect(container.querySelector("[class*='dropdownList']")).toBeNull();
		});

		it("autofocuses the search input on open", async () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			fireEvent.click(trigger);

			const search = container.querySelector("[data-testid='base-ref-search']") as HTMLInputElement;
			expect(search).not.toBeNull();
			await vi.waitFor(() => expect(document.activeElement).toBe(search));
		});

		it("filters refs by query across both Local and Remote groups", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			fireEvent.click(container.querySelector("[class*='dropdownTrigger']")!);

			const search = container.querySelector("[data-testid='base-ref-search']") as HTMLInputElement;
			fireEvent.input(search, { target: { value: "feature" } });

			const items = container.querySelectorAll("[data-testid='base-ref-item']");
			expect(items.length).toBe(1);
			expect(items[0].textContent).toContain("origin/feature/x");
		});

		it("shows an empty state when the query matches nothing", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			fireEvent.click(container.querySelector("[class*='dropdownTrigger']")!);

			const search = container.querySelector("[data-testid='base-ref-search']") as HTMLInputElement;
			fireEvent.input(search, { target: { value: "zzz-no-match" } });

			expect(container.querySelectorAll("[data-testid='base-ref-item']").length).toBe(0);
			expect(container.textContent).toContain("No refs match");
		});

		it("navigates with ArrowDown/ArrowUp and commits the highlighted ref on Enter", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			fireEvent.click(trigger);

			const search = container.querySelector("[data-testid='base-ref-search']") as HTMLInputElement;
			// BASE_REFS order: main (local), develop (local), origin/feature/x (remote).
			fireEvent.keyDown(search, { key: "ArrowDown" }); // -> main
			fireEvent.keyDown(search, { key: "ArrowDown" }); // -> develop
			fireEvent.keyDown(search, { key: "Enter" });

			expect(container.querySelector("[class*='dropdownList']")).toBeNull();
			expect(trigger.textContent).toContain("develop");
		});

		it("does not submit the dialog when Enter is used to commit a ref", () => {
			const handleCreate = vi.fn();
			const { container } = render(() => (
				<CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} onCreate={handleCreate} />
			));
			fireEvent.click(container.querySelector("[class*='dropdownTrigger']")!);

			const search = container.querySelector("[data-testid='base-ref-search']") as HTMLInputElement;
			fireEvent.keyDown(search, { key: "ArrowDown" });
			fireEvent.keyDown(search, { key: "Enter" });

			// Enter inside the search box must not bubble to the dialog's own Enter
			// handler and submit the (currently name-less) form.
			expect(handleCreate).not.toHaveBeenCalled();
		});

		it("Enter (or Space) on the focused trigger does not submit the dialog", () => {
			const handleCreate = vi.fn();
			const { container } = render(() => (
				<CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} onCreate={handleCreate} />
			));
			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "feature/new-thing" } });

			fireEvent.keyDown(container.querySelector("[class*='dropdownTrigger']")!, { key: "Enter" });

			expect(handleCreate).not.toHaveBeenCalled();
		});

		it("first Escape closes only the dropdown; a second closes the dialog", () => {
			const handleClose = vi.fn();
			const { container } = render(() => (
				<CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} onClose={handleClose} />
			));
			fireEvent.click(container.querySelector("[class*='dropdownTrigger']")!);
			expect(container.querySelector("[class*='dropdownList']")).not.toBeNull();

			fireEvent.keyDown(document, { key: "Escape" });
			expect(container.querySelector("[class*='dropdownList']")).toBeNull();
			expect(handleClose).not.toHaveBeenCalled();

			fireEvent.keyDown(document, { key: "Escape" });
			expect(handleClose).toHaveBeenCalledOnce();
		});

		it("stays open when a mousedown lands inside the list (e.g. on the search box)", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			fireEvent.click(container.querySelector("[class*='dropdownTrigger']")!);

			const search = container.querySelector("[data-testid='base-ref-search']") as HTMLInputElement;
			fireEvent.mouseDown(search);

			expect(container.querySelector("[class*='dropdownList']")).not.toBeNull();
		});

		it("navigates with ArrowUp back toward the top of the list", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			fireEvent.click(trigger);

			const search = container.querySelector("[data-testid='base-ref-search']") as HTMLInputElement;
			fireEvent.keyDown(search, { key: "ArrowDown" }); // -> main
			fireEvent.keyDown(search, { key: "ArrowDown" }); // -> develop
			fireEvent.keyDown(search, { key: "ArrowUp" }); // back to -> main
			fireEvent.keyDown(search, { key: "Enter" });

			expect(trigger.textContent).toContain("main");
		});

		it("Enter with no keyboard cursor moved is a no-op (does not close or change the value)", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			fireEvent.click(trigger);

			const search = container.querySelector("[data-testid='base-ref-search']") as HTMLInputElement;
			fireEvent.keyDown(search, { key: "Enter" });

			expect(container.querySelector("[class*='dropdownList']")).not.toBeNull();
			expect(trigger.textContent).toContain("main");
		});

		it("Tab closes the list", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			fireEvent.click(container.querySelector("[class*='dropdownTrigger']")!);

			const search = container.querySelector("[data-testid='base-ref-search']") as HTMLInputElement;
			fireEvent.keyDown(search, { key: "Tab" });

			expect(container.querySelector("[class*='dropdownList']")).toBeNull();
		});

		it("clicking an item after filtering selects it (index doesn't go stale when an earlier match drops out)", () => {
			// Regression test: filtering to "dev" removes "main" (index 0), shifting
			// "develop" from index 1 to index 0 within the filtered list. A click must
			// resolve against the option's live position, not a position captured when
			// the row was first rendered.
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			fireEvent.click(trigger);

			const search = container.querySelector("[data-testid='base-ref-search']") as HTMLInputElement;
			fireEvent.input(search, { target: { value: "dev" } });

			const developItem = Array.from(container.querySelectorAll("[class*='dropdownItem']")).find(
				(el) => el.textContent?.trim() === "develop",
			)!;
			fireEvent.click(developItem);

			expect(container.querySelector("[class*='dropdownList']")).toBeNull();
			expect(trigger.textContent).toContain("develop");
		});

		it("clicking a remote item survives all local matches being filtered out (index recomputed across the whole list, not just its own group)", () => {
			// Regression test: the remote group's index used to be computed once as
			// `filteredLocalRefs().length + i()` at render time. Filtering the local
			// group down to zero changes that offset without re-rendering the remote
			// row, so a stale offset must not cause the click to miss or select wrong.
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			fireEvent.click(trigger);

			const search = container.querySelector("[data-testid='base-ref-search']") as HTMLInputElement;
			fireEvent.input(search, { target: { value: "feature" } });

			const remoteItem = Array.from(container.querySelectorAll("[class*='dropdownItem']")).find((el) =>
				el.textContent?.includes("origin/feature/x"),
			)!;
			fireEvent.click(remoteItem);

			expect(container.querySelector("[class*='dropdownList']")).toBeNull();
			expect(trigger.textContent).toContain("origin/feature/x");
		});

		it("hovering an item after filtering moves the keyboard cursor to its live position, so Enter selects the hovered option", () => {
			// Regression test: onMouseEnter had the same stale-index bug as onClick —
			// covered separately since it drives a different code path (setSelectedIndex
			// feeding the keyboard Enter handler, not a direct onChange call).
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			fireEvent.click(trigger);

			const search = container.querySelector("[data-testid='base-ref-search']") as HTMLInputElement;
			fireEvent.input(search, { target: { value: "dev" } });

			const developItem = Array.from(container.querySelectorAll("[class*='dropdownItem']")).find(
				(el) => el.textContent?.trim() === "develop",
			)!;
			fireEvent.mouseEnter(developItem);
			fireEvent.keyDown(search, { key: "Enter" });

			expect(container.querySelector("[class*='dropdownList']")).toBeNull();
			expect(trigger.textContent).toContain("develop");
		});

		it("hovering an item moves the keyboard cursor onto it", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} baseRefs={BASE_REFS} />);
			fireEvent.click(container.querySelector("[class*='dropdownTrigger']")!);

			const developItem = Array.from(container.querySelectorAll("[data-testid='base-ref-item']")).find(
				(el) => el.textContent?.trim() === "develop",
			)!;
			fireEvent.mouseEnter(developItem);

			expect(developItem.classList.toString()).toContain("dropdownItemSelected");
		});
	});

	describe("keyboard submission", () => {
		it("creates via Enter on document, same as clicking Create", () => {
			const handleCreate = vi.fn();
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />);
			const input = container.querySelector("input[type='text']") as HTMLInputElement;

			fireEvent.input(input, { target: { value: "feature/new-thing" } });
			fireEvent.keyDown(document, { key: "Enter" });

			expect(handleCreate).toHaveBeenCalledWith({
				branchName: "feature/new-thing",
				createBranch: true,
				baseRef: "",
			});
		});

		it("does not create on Enter when the input is empty", () => {
			const handleCreate = vi.fn();
			render(() => <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />);

			fireEvent.keyDown(document, { key: "Enter" });

			expect(handleCreate).not.toHaveBeenCalled();
		});
	});

	describe("Escape handling", () => {
		it("closes the dialog via the shared modal stack", () => {
			const handleClose = vi.fn();
			render(() => <CreateWorktreeDialog {...defaultProps} onClose={handleClose} />);

			fireEvent.keyDown(document, { key: "Escape" });

			expect(handleClose).toHaveBeenCalledOnce();
		});

		it("stops handling Escape once the dialog is hidden (listener cleaned up)", () => {
			const handleClose = vi.fn();
			const [visible, setVisible] = createSignal(true);
			render(() => <CreateWorktreeDialog {...defaultProps} visible={visible()} onClose={handleClose} />);

			setVisible(false);
			fireEvent.keyDown(document, { key: "Escape" });

			expect(handleClose).not.toHaveBeenCalled();
		});
	});

	describe("re-entrancy while creating", () => {
		it("ignores a second Enter while onCreate is still pending, and disables the Create button", async () => {
			let resolveCreate: () => void = () => {};
			const handleCreate = vi.fn(
				() =>
					new Promise<void>((resolve) => {
						resolveCreate = resolve;
					}),
			);
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />);
			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "feature/new-thing" } });

			fireEvent.keyDown(document, { key: "Enter" });
			expect(handleCreate).toHaveBeenCalledTimes(1);

			const createBtn = container.querySelector(".primaryBtn") as HTMLButtonElement;
			await vi.waitFor(() => expect(createBtn.disabled).toBe(true));

			fireEvent.keyDown(document, { key: "Enter" });
			fireEvent.click(createBtn);
			expect(handleCreate).toHaveBeenCalledTimes(1);

			resolveCreate();
			await vi.waitFor(() => expect(createBtn.disabled).toBe(true)); // empty input still disables it
		});
	});

	describe("reset on open", () => {
		it("clears name, error and baseRef, and focuses the input when reopened", async () => {
			const [visible, setVisible] = createSignal(false);
			const { container } = render(() => (
				<CreateWorktreeDialog {...defaultProps} visible={visible()} baseRefs={BASE_REFS} />
			));

			setVisible(true);

			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			expect(input.value).toBe("");
			expect(container.querySelector(".error")).toBeNull();

			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			expect(trigger.textContent).toContain("main");

			await vi.waitFor(() => expect(document.activeElement).toBe(input));
		});

		it("resets state left over from a previous open (typed name, error, chosen baseRef)", async () => {
			const handleCreate = vi.fn();
			const [visible, setVisible] = createSignal(true);
			const { container } = render(() => (
				<CreateWorktreeDialog {...defaultProps} visible={visible()} baseRefs={BASE_REFS} onCreate={handleCreate} />
			));

			// Dirty the dialog: pick a non-default base ref, type an invalid name, trigger an error.
			const trigger = container.querySelector("[class*='dropdownTrigger']")!;
			fireEvent.click(trigger);
			const developItem = Array.from(container.querySelectorAll("[class*='dropdownItem']")).find(
				(el) => el.textContent?.trim() === "develop",
			)!;
			fireEvent.click(developItem);

			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "bad name" } });
			fireEvent.click(container.querySelector(".primaryBtn")!);
			expect(container.querySelector(".error")).not.toBeNull();

			// Close, then reopen — everything should be back to defaults. The dialog's
			// <Show> unmounts/remounts the DOM on each toggle, so re-query rather than
			// reuse the pre-toggle `input` reference (it now points at a detached node).
			setVisible(false);
			setVisible(true);

			const reopenedInput = container.querySelector("input[type='text']") as HTMLInputElement;
			expect(reopenedInput.value).toBe("");
			expect(container.querySelector(".error")).toBeNull();
			expect(container.querySelector("[class*='dropdownTrigger']")!.textContent).toContain("main");
			// Autofocus-on-reopen itself is covered by the dedicated test above; the
			// reopen's setTimeout(0) is cleaned up via onCleanup on the next effect run
			// or on unmount, so this test doesn't need to wait it out.
		});
	});

	describe("async onCreate rejection", () => {
		it("shows the error, clears isCreating, and keeps the dialog open", async () => {
			const handleCreate = vi.fn().mockRejectedValue(new Error("branch already exists on remote"));
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />);
			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "feature/new-thing" } });

			const createBtn = container.querySelector(".primaryBtn") as HTMLButtonElement;
			fireEvent.click(createBtn);

			await vi.waitFor(() => {
				expect(container.querySelector(".error")?.textContent).toContain("branch already exists on remote");
			});
			// Dialog stayed open (it has no visibility of its own — verify via the popover still being present)
			expect(container.querySelector(".popover")).not.toBeNull();
			// isCreating cleared: button is enabled again given the input still has text
			expect(createBtn.disabled).toBe(false);
		});
	});

	describe("empty branch list", () => {
		it("renders no branch items when existingBranches is empty", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} existingBranches={[]} />);
			expect(container.querySelectorAll("[class*='branchItem']").length).toBe(0);
		});

		it("renders no branch items when the query matches nothing", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "zzz-no-match" } });
			expect(container.querySelectorAll("[class*='branchItem']").length).toBe(0);
		});

		it("shows an empty-state message instead of a bare void when typing a brand-new branch name", () => {
			// Regression test: the branch list box has a fixed height (so it doesn't
			// resize while filtering — see the base ref dropdown tests below for the
			// matching bug), which means a zero-match state must render *something*
			// in that space, not a blank tinted box, for the common "type a brand new
			// branch name" flow.
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "zzz-no-match" } });

			const branchList = container.querySelector("[class*='branchList']")!;
			expect(branchList.textContent).toContain("No existing branches match");
		});
	});

	describe("branch list keyboard navigation", () => {
		it("ArrowDown skips a disabled (has-worktree) row and highlights the next one", () => {
			// defaultProps order: main (disabled), develop, feature/auth, fix/login-bug
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
			fireEvent.keyDown(document, { key: "ArrowDown" });

			const items = container.querySelectorAll("[data-testid='worktree-branch-item']");
			const highlighted = Array.from(items).find((el) => el.classList.toString().includes("branchItemHighlighted"));
			expect(highlighted?.textContent).toContain("develop");
		});

		it("Enter with no keyboard cursor still creates using the typed text", () => {
			const handleCreate = vi.fn();
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />);
			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "feature/new-thing" } });

			fireEvent.keyDown(document, { key: "Enter" });

			expect(handleCreate).toHaveBeenCalledWith({
				branchName: "feature/new-thing",
				createBranch: true,
				baseRef: "",
			});
		});

		it("Enter with a keyboard cursor accepts the highlighted branch instead of creating", () => {
			const handleCreate = vi.fn();
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />);

			fireEvent.keyDown(document, { key: "ArrowDown" }); // skips "main", lands on "develop"
			fireEvent.keyDown(document, { key: "Enter" });

			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			expect(input.value).toBe("develop");
			expect(handleCreate).not.toHaveBeenCalled();

			// A second Enter now creates, same as the mouse-click flow.
			fireEvent.keyDown(document, { key: "Enter" });
			expect(handleCreate).toHaveBeenCalledWith({ branchName: "develop", createBranch: false, baseRef: "" });
		});

		it("typing resets the keyboard cursor", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
			fireEvent.keyDown(document, { key: "ArrowDown" });
			expect(
				Array.from(container.querySelectorAll("[data-testid='worktree-branch-item']")).some((el) =>
					el.classList.toString().includes("branchItemHighlighted"),
				),
			).toBe(true);

			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "d" } });

			expect(
				Array.from(container.querySelectorAll("[data-testid='worktree-branch-item']")).some((el) =>
					el.classList.toString().includes("branchItemHighlighted"),
				),
			).toBe(false);
		});

		it("wraps the matched substring of each branch in a <mark>", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "feat" } });

			const mark = container.querySelector("[data-testid='worktree-branch-item'] mark");
			expect(mark).not.toBeNull();
			expect(mark!.textContent?.toLowerCase()).toBe("feat");
		});

		it("ArrowUp moves the cursor back up, and clears it once past the top", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
			fireEvent.keyDown(document, { key: "ArrowDown" }); // skips "main", lands on "develop"
			fireEvent.keyDown(document, { key: "ArrowDown" }); // -> "feature/auth"
			fireEvent.keyDown(document, { key: "ArrowUp" }); // back to -> "develop"

			let highlighted = Array.from(container.querySelectorAll("[data-testid='worktree-branch-item']")).find((el) =>
				el.classList.toString().includes("branchItemHighlighted"),
			);
			expect(highlighted?.textContent).toContain("develop");

			fireEvent.keyDown(document, { key: "ArrowUp" }); // past the top ("main" is disabled) -> no cursor
			highlighted = Array.from(container.querySelectorAll("[data-testid='worktree-branch-item']")).find((el) =>
				el.classList.toString().includes("branchItemHighlighted"),
			);
			expect(highlighted).toBeUndefined();
		});

		it("ArrowDown is a no-op when every visible branch already has a worktree", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
			const input = container.querySelector("input[type='text']") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "main" } }); // filters down to just the disabled "main" row

			fireEvent.keyDown(document, { key: "ArrowDown" });

			const highlighted = Array.from(container.querySelectorAll("[data-testid='worktree-branch-item']")).find((el) =>
				el.classList.toString().includes("branchItemHighlighted"),
			);
			expect(highlighted).toBeUndefined();
		});

		it("hovering a disabled row leaves the cursor alone; hovering an enabled row moves it", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
			const items = container.querySelectorAll("[data-testid='worktree-branch-item']");
			const mainItem = Array.from(items).find((el) => el.textContent?.includes("has worktree"))!;
			const developItem = Array.from(items).find((el) => el.textContent?.includes("develop"))!;

			fireEvent.mouseEnter(mainItem);
			expect(mainItem.classList.toString()).not.toContain("branchItemHighlighted");

			fireEvent.mouseEnter(developItem);
			expect(developItem.classList.toString()).toContain("branchItemHighlighted");
		});
	});

	describe("error dismissal outside the generate-name button", () => {
		it("clears the error when typing in the name input", () => {
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} />);
			const input = container.querySelector("input[type='text']") as HTMLInputElement;

			fireEvent.input(input, { target: { value: "bad name" } });
			fireEvent.click(container.querySelector(".primaryBtn")!);
			expect(container.querySelector(".error")).not.toBeNull();

			fireEvent.input(input, { target: { value: "bad name2" } });
			expect(container.querySelector(".error")).toBeNull();
		});

		it("clears the error when clicking a branch in the list", async () => {
			// "main" (has-worktree) and invalid-name errors both filter the list down
			// to nothing else clickable, so exercise the async-rejection error path
			// instead — "dev" is a substring only of "develop", which stays visible.
			const handleCreate = vi.fn().mockRejectedValue(new Error("boom"));
			const { container } = render(() => <CreateWorktreeDialog {...defaultProps} onCreate={handleCreate} />);
			const input = container.querySelector("input[type='text']") as HTMLInputElement;

			fireEvent.input(input, { target: { value: "dev" } });
			fireEvent.click(container.querySelector(".primaryBtn")!);
			await vi.waitFor(() => expect(container.querySelector(".error")).not.toBeNull());

			const developItem = container.querySelector("[data-testid='worktree-branch-item']")!;
			expect(developItem.textContent).toContain("develop");
			fireEvent.click(developItem);

			expect(container.querySelector(".error")).toBeNull();
		});
	});
});
