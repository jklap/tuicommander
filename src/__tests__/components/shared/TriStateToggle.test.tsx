import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { TriStateToggle } from "../../../components/shared/TriStateToggle";

describe("TriStateToggle", () => {
	it("renders a single checkbox-role switch, not a radiogroup", () => {
		const { getByRole, queryByRole } = render(() => (
			<TriStateToggle value={null} onChange={vi.fn()} label="Hide Draft PRs" inherited={false} />
		));
		expect(getByRole("checkbox", { name: "Hide Draft PRs" })).toBeTruthy();
		expect(queryByRole("radiogroup")).toBeFalsy();
		expect(queryByRole("radio")).toBeFalsy();
	});

	it("encodes value as aria-checked, with mixed for inherit (null)", () => {
		const [value, setValue] = createSignal<boolean | null>(null);
		const { getByRole } = render(() => (
			<TriStateToggle value={value()} onChange={vi.fn()} label="Hide Draft PRs" inherited={false} />
		));
		expect(getByRole("checkbox").getAttribute("aria-checked")).toBe("mixed");

		setValue(true);
		expect(getByRole("checkbox").getAttribute("aria-checked")).toBe("true");

		setValue(false);
		expect(getByRole("checkbox").getAttribute("aria-checked")).toBe("false");
	});

	it("reflects value as a data-state attribute for styling", () => {
		const [value, setValue] = createSignal<boolean | null>(null);
		const { getByRole } = render(() => (
			<TriStateToggle value={value()} onChange={vi.fn()} label="Hide Draft PRs" inherited={false} />
		));
		expect(getByRole("checkbox").getAttribute("data-state")).toBe("global");

		setValue(true);
		expect(getByRole("checkbox").getAttribute("data-state")).toBe("on");

		setValue(false);
		expect(getByRole("checkbox").getAttribute("data-state")).toBe("off");
	});

	it("clicking cycles Global -> On -> Off -> Global", () => {
		const [value, setValue] = createSignal<boolean | null>(null);
		const { getByRole } = render(() => (
			<TriStateToggle value={value()} onChange={setValue} label="Hide Draft PRs" inherited={false} />
		));
		const el = getByRole("checkbox");

		fireEvent.click(el);
		expect(value()).toBe(true);

		fireEvent.click(el);
		expect(value()).toBe(false);

		fireEvent.click(el);
		expect(value()).toBeNull();
	});

	it("Space and Enter also cycle the state", () => {
		const [value, setValue] = createSignal<boolean | null>(null);
		const { getByRole } = render(() => (
			<TriStateToggle value={value()} onChange={setValue} label="Hide Draft PRs" inherited={false} />
		));
		const el = getByRole("checkbox");

		fireEvent.keyDown(el, { key: " " });
		expect(value()).toBe(true);

		fireEvent.keyDown(el, { key: "Enter" });
		expect(value()).toBe(false);
	});

	it("shows the resolved global value as a hint only while value is null", () => {
		const [value, setValue] = createSignal<boolean | null>(null);
		const { getByRole, queryByText } = render(() => (
			<TriStateToggle value={value()} onChange={vi.fn()} label="Hide Draft PRs" inherited={true} />
		));
		expect(getByRole("checkbox").parentElement?.textContent).toContain("Use global default: On");
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
		expect(getByRole("checkbox").getAttribute("title")).toBe("Draft PRs: Hide");
	});

	it("title attribute names the current state for discoverability without visible segment text", () => {
		const [value, setValue] = createSignal<boolean | null>(null);
		const { getByRole } = render(() => (
			<TriStateToggle value={value()} onChange={vi.fn()} label="Hide Draft PRs" inherited={true} />
		));
		expect(getByRole("checkbox").getAttribute("title")).toBe("Hide Draft PRs: Global");

		setValue(false);
		expect(getByRole("checkbox").getAttribute("title")).toBe("Hide Draft PRs: Off");
	});
});
