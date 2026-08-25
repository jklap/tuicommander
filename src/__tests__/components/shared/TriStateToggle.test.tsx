import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { TriStateToggle } from "../../../components/shared/TriStateToggle";

describe("TriStateToggle", () => {
	it("renders a radiogroup of three radios in Off / Global / On order", () => {
		const { getByRole } = render(() => (
			<TriStateToggle value={null} onChange={vi.fn()} label="Hide Draft PRs" inherited={false} />
		));
		const group = getByRole("radiogroup", { name: "Hide Draft PRs" });
		const radios = group.querySelectorAll('[role="radio"]');
		expect(radios).toHaveLength(3);
		expect(radios[0].textContent).toBe("Off");
		expect(radios[1].textContent).toBe("Global");
		expect(radios[2].textContent).toBe("On");
	});

	it("marks the segment matching `value` as checked, and only that one", () => {
		const { getByRole } = render(() => (
			<TriStateToggle value={true} onChange={vi.fn()} label="Hide Draft PRs" inherited={false} />
		));
		const group = getByRole("radiogroup");
		const radios = Array.from(group.querySelectorAll('[role="radio"]'));
		expect(radios.map((r) => r.getAttribute("aria-checked"))).toEqual(["false", "false", "true"]);
	});

	it("clicking a segment calls onChange with that segment's value", () => {
		const onChange = vi.fn();
		const { getByRole } = render(() => (
			<TriStateToggle value={null} onChange={onChange} label="Hide Draft PRs" inherited={false} />
		));
		const group = getByRole("radiogroup");
		const [off, global_, on] = Array.from(group.querySelectorAll('[role="radio"]'));
		fireEvent.click(on);
		expect(onChange).toHaveBeenLastCalledWith(true);
		fireEvent.click(off);
		expect(onChange).toHaveBeenLastCalledWith(false);
		fireEvent.click(global_);
		expect(onChange).toHaveBeenLastCalledWith(null);
	});

	it("ArrowRight/ArrowLeft move and select the adjacent segment, clamped at the ends", () => {
		const onChange = vi.fn();
		const { getByRole } = render(() => (
			<TriStateToggle value={false} onChange={onChange} label="Hide Draft PRs" inherited={false} />
		));
		const group = getByRole("radiogroup");
		fireEvent.keyDown(group, { key: "ArrowLeft" });
		// already at the leftmost (Off) segment — clamped, no spurious call
		expect(onChange).not.toHaveBeenCalled();
		fireEvent.keyDown(group, { key: "ArrowUp" });
		expect(onChange).not.toHaveBeenCalled();
		fireEvent.keyDown(group, { key: "ArrowRight" });
		expect(onChange).toHaveBeenLastCalledWith(null);
	});

	it("moves DOM focus to the newly selected segment on arrow key (roving tabindex)", () => {
		// A plain checked/onChange assertion isn't enough here: without moving focus,
		// the :focus-visible outline stays on the segment you started on while the
		// highlighted/checked segment jumps elsewhere — a real desync bug.
		const [value, setValue] = createSignal<boolean | null>(false);
		const { getByRole } = render(() => (
			<TriStateToggle value={value()} onChange={setValue} label="Hide Draft PRs" inherited={false} />
		));
		const group = getByRole("radiogroup");
		const radios = Array.from(group.querySelectorAll('[role="radio"]')) as HTMLElement[];
		radios[0].focus();
		expect(document.activeElement).toBe(radios[0]);

		fireEvent.keyDown(group, { key: "ArrowRight" });
		expect(value()).toBeNull();
		expect(document.activeElement).toBe(radios[1]);

		fireEvent.keyDown(group, { key: "ArrowRight" });
		expect(value()).toBe(true);
		expect(document.activeElement).toBe(radios[2]);
	});

	it("shows the resolved global value as a hint only while value is null", () => {
		const [value, setValue] = createSignal<boolean | null>(null);
		const { getByRole, queryByText } = render(() => (
			<TriStateToggle value={value()} onChange={vi.fn()} label="Hide Draft PRs" inherited={true} />
		));
		expect(getByRole("radiogroup").parentElement?.textContent).toContain("Use global default: On");
		expect(queryByText(/Use global default/)).toBeTruthy();

		setValue(false);
		expect(queryByText(/Use global default/)).toBeFalsy();
	});

	it("supports custom on/off labels for framing like Show/Hide", () => {
		const { getByRole } = render(() => (
			<TriStateToggle
				value={true}
				onChange={vi.fn()}
				label="Draft PRs"
				inherited={false}
				onLabel="Hide"
				offLabel="Show"
			/>
		));
		const group = getByRole("radiogroup");
		const radios = Array.from(group.querySelectorAll('[role="radio"]'));
		expect(radios[0].textContent).toBe("Show");
		expect(radios[2].textContent).toBe("Hide");
	});

	it("gives the checked segment tabIndex 0 and the others -1 (roving tabindex)", () => {
		const { getByRole } = render(() => (
			<TriStateToggle value={null} onChange={vi.fn()} label="Hide Draft PRs" inherited={false} />
		));
		const group = getByRole("radiogroup");
		const radios = Array.from(group.querySelectorAll('[role="radio"]'));
		expect(radios.map((r) => r.getAttribute("tabindex"))).toEqual(["-1", "0", "-1"]);
	});
});
