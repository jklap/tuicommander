import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { IndicatorEditorDialog } from "../../indicators/IndicatorEditorDialog";
import { settingsStore } from "../../stores/settings";

describe("IndicatorEditorDialog", () => {
	afterEach(() => {
		settingsStore.resetAllIndicators();
	});

	it("renders nothing for a null indicator id", () => {
		const { container } = render(() => <IndicatorEditorDialog indicatorId={null} onClose={vi.fn()} />);
		expect(container.querySelector(".overlay")).toBeNull();
	});

	it("renders nothing for an indicator id the registry doesn't know about", () => {
		// Defensive: indicatorId is caller-supplied (UiLegend passes a live registry
		// entry's own id today, but the prop type is a bare string) — a typo or a
		// stale id from a future registry change must not crash the dialog.
		const { container } = render(() => <IndicatorEditorDialog indicatorId="not.a.real.indicator" onClose={vi.fn()} />);
		expect(container.querySelector(".overlay")).toBeNull();
	});

	it("shows only the color section and the glyph preview for a diffStat entry", () => {
		const { container, queryByText } = render(() => (
			<IndicatorEditorDialog indicatorId="diffStat.additions" onClose={vi.fn()} />
		));
		expect(container.querySelector(".colorSwatch")).not.toBeNull();
		expect(container.querySelectorAll(".swatch").length).toBe(0);
		expect(queryByText("Pulse")).toBeNull();
		expect(container.textContent).toContain("+N");
	});

	it("shows the indicator's label as the header and its description in the body", () => {
		const { getByRole, getByText } = render(() => (
			<IndicatorEditorDialog indicatorId="terminal.busy" onClose={vi.fn()} />
		));
		expect(getByRole("heading", { name: "Busy" })).toBeTruthy();
		expect(getByText("Producing output")).toBeTruthy();
	});

	it("shows color, icon, and animation sections for a fully-capable entry", () => {
		const { container, getByText } = render(() => (
			<IndicatorEditorDialog indicatorId="terminal.busy" onClose={vi.fn()} />
		));
		expect(container.querySelector(".colorSwatch")).not.toBeNull();
		expect(container.querySelectorAll(".swatch").length).toBeGreaterThan(0);
		expect(getByText("Pulse")).toBeTruthy();
	});

	it("shows only the color section for a color-only entry (a Tab Type row)", () => {
		const { container, queryByText } = render(() => (
			<IndicatorEditorDialog indicatorId="tabType.diff" onClose={vi.fn()} />
		));
		expect(container.querySelector(".colorSwatch")).not.toBeNull();
		expect(container.querySelectorAll(".swatch").length).toBe(0);
		expect(queryByText("Pulse")).toBeNull();
	});

	it("shows only the icon section for an icon-only entry (sidebar.shell)", () => {
		const { container, queryByText } = render(() => (
			<IndicatorEditorDialog indicatorId="sidebar.shell" onClose={vi.fn()} />
		));
		expect(container.querySelector(".colorSwatch")).toBeNull();
		expect(container.querySelectorAll(".swatch").length).toBeGreaterThan(0);
		expect(queryByText("Pulse")).toBeNull();
	});

	it("clicking a color preset calls setIndicatorColor for the open indicator", () => {
		const { getByTitle } = render(() => <IndicatorEditorDialog indicatorId="terminal.busy" onClose={vi.fn()} />);
		fireEvent.click(getByTitle("Blue"));
		expect(settingsStore.state.indicatorOverrides).toEqual([{ id: "terminal.busy", color: "#4A9EFF" }]);
	});

	it("clicking an icon swatch calls setIndicatorIcon for the open indicator", () => {
		const { getByTitle } = render(() => <IndicatorEditorDialog indicatorId="terminal.busy" onClose={vi.fn()} />);
		fireEvent.click(getByTitle("ring"));
		expect(settingsStore.state.indicatorOverrides).toEqual([{ id: "terminal.busy", icon: "ring" }]);
	});

	it("clicking an animation option calls setIndicatorAnimation for the open indicator", () => {
		const { getByText } = render(() => <IndicatorEditorDialog indicatorId="terminal.busy" onClose={vi.fn()} />);
		fireEvent.click(getByText("Blink"));
		expect(settingsStore.state.indicatorOverrides).toEqual([{ id: "terminal.busy", animation: "blink" }]);
	});

	it("restricts the animation section to a badge entry's narrower list (no Glow/Spin for pr.conflict)", () => {
		const { queryByText } = render(() => <IndicatorEditorDialog indicatorId="pr.conflict" onClose={vi.fn()} />);
		expect(queryByText("Pulse")).toBeTruthy();
		expect(queryByText("Glow")).toBeNull();
		expect(queryByText("Spin")).toBeNull();
	});

	it("clicking 'No color' clears only the color, leaving a sibling icon override standing", () => {
		settingsStore.setIndicatorColor("terminal.busy", "#ff00ff");
		settingsStore.setIndicatorIcon("terminal.busy", "ring");
		const { getByTitle } = render(() => <IndicatorEditorDialog indicatorId="terminal.busy" onClose={vi.fn()} />);

		fireEvent.click(getByTitle("No color"));

		expect(settingsStore.state.indicatorOverrides).toEqual([{ id: "terminal.busy", icon: "ring" }]);
	});

	it("hides 'Reset to default' when the indicator has no override", () => {
		const { queryByText } = render(() => <IndicatorEditorDialog indicatorId="terminal.busy" onClose={vi.fn()} />);
		expect(queryByText("Reset to default")).toBeNull();
	});

	it("'Reset to default' clears every field of the open indicator's override", () => {
		settingsStore.setIndicatorColor("terminal.busy", "#ff00ff");
		settingsStore.setIndicatorIcon("terminal.busy", "ring");
		const { getByText } = render(() => <IndicatorEditorDialog indicatorId="terminal.busy" onClose={vi.fn()} />);

		fireEvent.click(getByText("Reset to default"));

		expect(settingsStore.state.indicatorOverrides).toEqual([]);
	});

	it("'Reset to default' does not affect a different indicator's override", () => {
		settingsStore.setIndicatorColor("terminal.busy", "#ff00ff");
		settingsStore.setIndicatorColor("pr.conflict", "#00ff00");
		const { getByText } = render(() => <IndicatorEditorDialog indicatorId="terminal.busy" onClose={vi.fn()} />);

		fireEvent.click(getByText("Reset to default"));

		expect(settingsStore.state.indicatorOverrides).toEqual([{ id: "pr.conflict", color: "#00ff00" }]);
	});

	it("clicking overlay backdrop calls onClose", () => {
		const onClose = vi.fn();
		const { container } = render(() => <IndicatorEditorDialog indicatorId="terminal.busy" onClose={onClose} />);
		fireEvent.click(container.querySelector(".overlay")!);
		expect(onClose).toHaveBeenCalled();
	});

	it("Escape key calls onClose", () => {
		const onClose = vi.fn();
		render(() => <IndicatorEditorDialog indicatorId="terminal.busy" onClose={onClose} />);
		fireEvent.keyDown(document, { key: "Escape" });
		expect(onClose).toHaveBeenCalled();
	});

	it("the Close button calls onClose", () => {
		const onClose = vi.fn();
		const { getByText } = render(() => <IndicatorEditorDialog indicatorId="terminal.busy" onClose={onClose} />);
		fireEvent.click(getByText("Close"));
		expect(onClose).toHaveBeenCalled();
	});
});
