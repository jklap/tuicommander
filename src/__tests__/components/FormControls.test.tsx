import { fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { Input } from "../../components/ui/Input";
import { SegmentedControl } from "../../components/ui/SegmentedControl";
import { Select } from "../../components/ui/Select";
import { Textarea } from "../../components/ui/Textarea";

describe("shared form controls", () => {
	// Catches: compact being dropped (a 36px field beside 20px buttons) or leaking onto the DOM as an attribute.
	it("adds the compact size class without leaking the prop", () => {
		render(() => (
			<>
				<Select aria-label="Plain">
					<option>a</option>
				</Select>
				<Select compact aria-label="Dense">
					<option>a</option>
				</Select>
				<Input compact aria-label="Dense input" />
			</>
		));
		const plain = screen.getByRole("combobox", { name: "Plain" });
		const dense = screen.getByRole("combobox", { name: "Dense" });
		expect(dense.className.split(" ").length).toBe(plain.className.split(" ").length + 1);
		expect(dense).not.toHaveAttribute("compact");
		expect(screen.getByRole("textbox", { name: "Dense input" })).not.toHaveAttribute("compact");
	});

	// Catches: Input stripping `compact` from the DOM props but never adding the size class, so Error Log search,
	// Branches create/rename and Agents env-flag fields stay 36px beside 22px neighbours.
	it("adds the compact size class to Input and keeps a caller class", () => {
		render(() => (
			<>
				<Input aria-label="Plain" class="mine" />
				<Input compact aria-label="Dense" class="mine" />
			</>
		));
		const plain = screen.getByRole("textbox", { name: "Plain" });
		const dense = screen.getByRole("textbox", { name: "Dense" });
		expect(dense.classList.length).toBe(plain.classList.length + 1);
		expect(dense).toHaveClass("mine");
	});

	// Catches: a wrapper freezing reactive props or losing native ref/event targets and form values.
	it("preserves reactive selection, native refs, names and change targets", () => {
		const [value, setValue] = createSignal("board");
		const [disabled, setDisabled] = createSignal(false);
		const [invalid, setInvalid] = createSignal(true);
		const changed = vi.fn();
		let element: HTMLSelectElement | undefined;
		render(() => (
			<form aria-label="Views">
				<Select
					aria-label="View"
					name="view"
					ref={(node) => {
						element = node;
					}}
					value={value()}
					disabled={disabled()}
					class="caller-layout"
					classList={{ invalid: invalid() }}
					onChange={(event) => changed(event.currentTarget.value)}
				>
					<option value="list">List</option>
					<option value="board">Board</option>
				</Select>
			</form>
		));
		const select = screen.getByRole("combobox", { name: "View" });
		expect(element).toBe(select);
		expect(select).toHaveValue("board");
		setValue("list");
		expect(select).toHaveValue("list");
		fireEvent.change(select, { target: { value: "board" } });
		expect(changed).toHaveBeenCalledWith("board");
		expect(new FormData(screen.getByRole("form", { name: "Views" }) as HTMLFormElement).get("view")).toBe("board");
		expect(select).toHaveClass("caller-layout", "invalid");
		setInvalid(false);
		expect(select).not.toHaveClass("invalid");
		setDisabled(true);
		expect(select).toBeDisabled();
	});

	// Catches: flattening selected children into a single value, breaking multi-select task fields.
	it("preserves multiple options selected through child attributes", () => {
		render(() => (
			<Select multiple aria-label="Labels">
				<option value="a" selected>
					A
				</option>
				<option value="b">B</option>
				<option value="c" selected>
					C
				</option>
			</Select>
		));
		const select = screen.getByRole("listbox", { name: "Labels" }) as HTMLSelectElement;
		expect(Array.from(select.selectedOptions, (option) => option.value)).toEqual(["a", "c"]);
	});

	// Catches: dropping input events, constraints, or reactive values while forwarding text controls.
	it("keeps input and textarea edits and reactive values at the native boundary", () => {
		const [text, setText] = createSignal("draft");
		render(() => (
			<>
				<Input
					aria-label="Title"
					name="title"
					required
					value={text()}
					onInput={(event) => setText(event.currentTarget.value)}
				/>
				<Textarea aria-label="Description" value={text()} readOnly />
			</>
		));
		const input = screen.getByRole("textbox", { name: "Title" });
		fireEvent.input(input, { target: { value: "edited" } });
		expect(input).toBeRequired();
		expect(screen.getByRole("textbox", { name: "Description" })).toHaveValue("edited");
		expect(screen.getByRole("textbox", { name: "Description" })).toHaveAttribute("readonly");
	});

	// Catches: keyboard navigation choosing disabled segments or leaving focus and selection out of sync.
	it("switches views with clicks and arrow/Home/End keys, skipping disabled choices", () => {
		const [view, setView] = createSignal("list");
		render(() => (
			<SegmentedControl
				aria-label="Tasks view"
				value={view()}
				onChange={setView}
				options={[
					{ id: "list", label: "List" },
					{ id: "board", label: "Board", disabled: true },
					{ id: "roadmap", label: "Roadmap" },
				]}
			/>
		));
		const list = screen.getByRole("tab", { name: "List" });
		const roadmap = screen.getByRole("tab", { name: "Roadmap" });
		fireEvent.keyDown(list, { key: "ArrowRight" });
		expect(roadmap).toHaveAttribute("aria-selected", "true");
		expect(roadmap).toHaveFocus();
		fireEvent.keyDown(roadmap, { key: "ArrowRight" });
		expect(list).toHaveFocus();
		fireEvent.keyDown(list, { key: "End" });
		expect(roadmap).toHaveFocus();
		fireEvent.keyDown(roadmap, { key: "Home" });
		expect(list).toHaveAttribute("tabindex", "0");
		fireEvent.click(roadmap);
		expect(roadmap).toHaveAttribute("aria-selected", "true");
	});
});
