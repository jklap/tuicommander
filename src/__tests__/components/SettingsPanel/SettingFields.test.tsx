import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import {
	SettingInput,
	SettingSelect,
	SettingSlider,
	SettingToggle,
} from "../../../components/SettingsPanel/SettingFields";

/**
 * `SettingFields.tsx` is the shared building block for every settings tab
 * (including `SelectionTab`'s "Double-click performs" / "Word boundaries"
 * selects) but had no direct
 * test file — it was only exercised incidentally through whichever tab
 * happened to render it. In particular `SettingSlider`'s `onCommit` (fired
 * on the DOM `change` event, distinct from the live-drag `onChange`/`onInput`)
 * has exactly one caller in the whole app (`NotificationsTab.tsx`'s sound
 * preview) and had no coverage of its own at all.
 */
describe("SettingToggle", () => {
	it("renders its label and hint, and reflects the checked prop", () => {
		const { getByText, container } = render(() => (
			<SettingToggle checked={true} onChange={() => {}} label="Enable thing" hint="Explains the thing" />
		));
		expect(getByText("Enable thing")).toBeTruthy();
		expect(getByText("Explains the thing")).toBeTruthy();
		expect((container.querySelector('input[type="checkbox"]') as HTMLInputElement).checked).toBe(true);
	});

	it("calls onChange with the new checked state", () => {
		const onChange = vi.fn();
		const { container } = render(() => <SettingToggle checked={false} onChange={onChange} label="Enable thing" />);
		const checkbox = container.querySelector('input[type="checkbox"]') as HTMLInputElement;
		fireEvent.click(checkbox);
		expect(onChange).toHaveBeenCalledWith(true);
	});

	it("omits the hint paragraph when none is given", () => {
		const { container } = render(() => <SettingToggle checked={false} onChange={() => {}} label="Enable thing" />);
		expect(container.querySelector("p")).toBeNull();
	});
});

describe("SettingSelect", () => {
	const options = [
		{ value: "a", label: "Option A" },
		{ value: "b", label: "Option B" },
	];

	it("renders every option and the current value", () => {
		const { container } = render(() => (
			<SettingSelect label="Pick one" value="b" onChange={() => {}} options={options} />
		));
		const select = container.querySelector("select") as HTMLSelectElement;
		expect(Array.from(select.options).map((o) => o.value)).toEqual(["a", "b"]);
		expect(select.value).toBe("b");
	});

	it("calls onChange with the selected value", () => {
		const onChange = vi.fn();
		const { container } = render(() => (
			<SettingSelect label="Pick one" value="a" onChange={onChange} options={options} />
		));
		const select = container.querySelector("select") as HTMLSelectElement;
		fireEvent.change(select, { target: { value: "b" } });
		expect(onChange).toHaveBeenCalledWith("b");
	});
});

describe("SettingSlider", () => {
	it("renders the formatted value using formatValue when provided", () => {
		const { getByText } = render(() => (
			<SettingSlider label="Volume" value={40} onChange={() => {}} min={0} max={100} formatValue={(v) => `${v}%`} />
		));
		expect(getByText("40%")).toBeTruthy();
	});

	it("falls back to value+suffix when no formatValue is given", () => {
		const { getByText } = render(() => (
			<SettingSlider label="Size" value={12} onChange={() => {}} min={0} max={20} suffix="px" />
		));
		expect(getByText("12px")).toBeTruthy();
	});

	it("calls onChange live (on input) as the slider is dragged", () => {
		const onChange = vi.fn();
		const { container } = render(() => (
			<SettingSlider label="Volume" value={40} onChange={onChange} min={0} max={100} />
		));
		const range = container.querySelector('input[type="range"]') as HTMLInputElement;
		fireEvent.input(range, { target: { value: "55" } });
		expect(onChange).toHaveBeenCalledWith(55);
	});

	it("calls onCommit only once the drag is released (DOM change), not on every input", () => {
		const onChange = vi.fn();
		const onCommit = vi.fn();
		const { container } = render(() => (
			<SettingSlider label="Volume" value={40} onChange={onChange} onCommit={onCommit} min={0} max={100} />
		));
		const range = container.querySelector('input[type="range"]') as HTMLInputElement;
		fireEvent.input(range, { target: { value: "55" } });
		expect(onCommit).not.toHaveBeenCalled();
		fireEvent.change(range, { target: { value: "55" } });
		expect(onCommit).toHaveBeenCalledWith(55);
	});
});

describe("SettingInput", () => {
	it("renders the current value and placeholder", () => {
		const { container } = render(() => (
			<SettingInput label="Name" value="" onInput={() => {}} placeholder="e.g. Alice" />
		));
		const input = container.querySelector("input") as HTMLInputElement;
		expect(input.placeholder).toBe("e.g. Alice");
		expect(input.type).toBe("text");
	});

	it("calls onInput with the new text", () => {
		const onInput = vi.fn();
		const { container } = render(() => <SettingInput label="Name" value="" onInput={onInput} />);
		const input = container.querySelector("input") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "Alice" } });
		expect(onInput).toHaveBeenCalledWith("Alice");
	});

	it("supports the password input type", () => {
		const { container } = render(() => <SettingInput label="Token" value="" onInput={() => {}} type="password" />);
		expect((container.querySelector("input") as HTMLInputElement).type).toBe("password");
	});
});
