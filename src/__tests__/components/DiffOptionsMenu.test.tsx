import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DiffOptionsMenu } from "../../components/shared/DiffOptionsMenu";

const {
	mockSetDiffIgnoreLeadingWhitespace,
	mockSetDiffIgnoreTrailingWhitespace,
	mockSetDiffIgnoreWhitespaceAmount,
	mockSetDiffIgnoreCase,
	mockSetDiffSoftWrap,
	settingsState,
	uiState,
} = vi.hoisted(() => ({
	mockSetDiffIgnoreLeadingWhitespace: vi.fn(),
	mockSetDiffIgnoreTrailingWhitespace: vi.fn(),
	mockSetDiffIgnoreWhitespaceAmount: vi.fn(),
	mockSetDiffIgnoreCase: vi.fn(),
	mockSetDiffSoftWrap: vi.fn(),
	settingsState: {
		diffIgnoreLeadingWhitespace: false,
		diffIgnoreTrailingWhitespace: false,
		diffIgnoreWhitespaceAmount: false,
		diffIgnoreCase: false,
	},
	uiState: { diffSoftWrap: false },
}));

vi.mock("../../stores/settings", () => ({
	settingsStore: {
		get state() {
			return settingsState;
		},
		setDiffIgnoreLeadingWhitespace: mockSetDiffIgnoreLeadingWhitespace,
		setDiffIgnoreTrailingWhitespace: mockSetDiffIgnoreTrailingWhitespace,
		setDiffIgnoreWhitespaceAmount: mockSetDiffIgnoreWhitespaceAmount,
		setDiffIgnoreCase: mockSetDiffIgnoreCase,
	},
}));

vi.mock("../../stores/ui", () => ({
	uiStore: {
		get state() {
			return uiState;
		},
		setDiffSoftWrap: mockSetDiffSoftWrap,
	},
}));

describe("DiffOptionsMenu", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		settingsState.diffIgnoreLeadingWhitespace = false;
		settingsState.diffIgnoreTrailingWhitespace = false;
		settingsState.diffIgnoreWhitespaceAmount = false;
		settingsState.diffIgnoreCase = false;
		uiState.diffSoftWrap = false;
	});

	it("the panel starts closed", () => {
		const { queryByTestId } = render(() => <DiffOptionsMenu />);
		expect(queryByTestId("diff-options-panel")).toBeNull();
	});

	it("clicking the trigger opens the panel with all 5 checkboxes", () => {
		const { getByTestId } = render(() => <DiffOptionsMenu />);
		fireEvent.click(getByTestId("diff-options-trigger"));
		const panel = getByTestId("diff-options-panel");
		expect(panel.querySelectorAll("input[type='checkbox']").length).toBe(5);
	});

	it("clicking the trigger again closes the panel", () => {
		const { getByTestId, queryByTestId } = render(() => <DiffOptionsMenu />);
		const trigger = getByTestId("diff-options-trigger");
		fireEvent.click(trigger);
		expect(queryByTestId("diff-options-panel")).not.toBeNull();
		fireEvent.click(trigger);
		expect(queryByTestId("diff-options-panel")).toBeNull();
	});

	it("closes on Escape", () => {
		const { getByTestId, queryByTestId } = render(() => <DiffOptionsMenu />);
		fireEvent.click(getByTestId("diff-options-trigger"));
		fireEvent.keyDown(document, { key: "Escape" });
		expect(queryByTestId("diff-options-panel")).toBeNull();
	});

	it("closes on click outside", async () => {
		const { getByTestId, queryByTestId } = render(() => (
			<div>
				<div data-testid="outside">Outside</div>
				<DiffOptionsMenu />
			</div>
		));
		fireEvent.click(getByTestId("diff-options-trigger"));
		expect(queryByTestId("diff-options-panel")).not.toBeNull();
		await vi.waitFor(() => {
			fireEvent.click(document.body);
			expect(queryByTestId("diff-options-panel")).toBeNull();
		});
	});

	it("toggling each checkbox calls its own store setter with the new value", () => {
		const { getByTestId } = render(() => <DiffOptionsMenu />);
		fireEvent.click(getByTestId("diff-options-trigger"));
		const panel = getByTestId("diff-options-panel");
		const checkboxes = Array.from(panel.querySelectorAll("input[type='checkbox']")) as HTMLInputElement[];

		fireEvent.click(checkboxes[0]);
		expect(mockSetDiffIgnoreLeadingWhitespace).toHaveBeenCalledWith(true);
		fireEvent.click(checkboxes[1]);
		expect(mockSetDiffIgnoreTrailingWhitespace).toHaveBeenCalledWith(true);
		fireEvent.click(checkboxes[2]);
		expect(mockSetDiffIgnoreWhitespaceAmount).toHaveBeenCalledWith(true);
		fireEvent.click(checkboxes[3]);
		expect(mockSetDiffIgnoreCase).toHaveBeenCalledWith(true);
		fireEvent.click(checkboxes[4]);
		expect(mockSetDiffSoftWrap).toHaveBeenCalledWith(true);
	});

	it("shows an active indicator on the trigger when any option is on", () => {
		settingsState.diffIgnoreCase = true;
		const { getByTestId } = render(() => <DiffOptionsMenu />);
		expect(getByTestId("diff-options-trigger").className).toContain("triggerActive");
	});

	it("has no active indicator when every option is off", () => {
		const { getByTestId } = render(() => <DiffOptionsMenu />);
		expect(getByTestId("diff-options-trigger").className).not.toContain("triggerActive");
	});
});
