import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import { fireEvent, render } from "@solidjs/testing-library";

const { mockSetShowBlockTimestamps, mockSetShowBlockMarks, mockSetShowPromptMarks, mockSetBlockFoldingEnabled } =
	vi.hoisted(() => ({
		mockSetShowBlockTimestamps: vi.fn(),
		mockSetShowBlockMarks: vi.fn(),
		mockSetShowPromptMarks: vi.fn(),
		mockSetBlockFoldingEnabled: vi.fn(),
	}));

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		state: {
			showBlockTimestamps: true,
			showBlockMarks: true,
			showPromptMarks: false,
			blockFoldingEnabled: true,
		},
		setShowBlockTimestamps: mockSetShowBlockTimestamps,
		setShowBlockMarks: mockSetShowBlockMarks,
		setShowPromptMarks: mockSetShowPromptMarks,
		setBlockFoldingEnabled: mockSetBlockFoldingEnabled,
	},
}));

import { TerminalTab } from "../../../components/SettingsPanel/tabs/TerminalTab";

describe("TerminalTab", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("renders the Blocks heading", () => {
		const { container } = render(() => <TerminalTab />);
		const headings = Array.from(container.querySelectorAll("h3")).map((h) => h.textContent);
		expect(headings).toContain("Blocks");
	});

	it("shows all four toggles with the correct checked state", () => {
		const { container } = render(() => <TerminalTab />);
		const checkboxes = Array.from(container.querySelectorAll("input[type=checkbox]")) as HTMLInputElement[];
		expect(checkboxes).toHaveLength(4);
		expect(checkboxes.map((cb) => cb.checked)).toEqual([true, true, false, true]);
	});

	it("calls setShowBlockTimestamps when its toggle changes", () => {
		const { container } = render(() => <TerminalTab />);
		const checkboxes = container.querySelectorAll("input[type=checkbox]");
		fireEvent.change(checkboxes[0], { target: { checked: false } });
		expect(mockSetShowBlockTimestamps).toHaveBeenCalledWith(false);
	});

	it("calls setShowBlockMarks when its toggle changes", () => {
		const { container } = render(() => <TerminalTab />);
		const checkboxes = container.querySelectorAll("input[type=checkbox]");
		fireEvent.change(checkboxes[1], { target: { checked: false } });
		expect(mockSetShowBlockMarks).toHaveBeenCalledWith(false);
	});

	it("calls setShowPromptMarks when its toggle changes", () => {
		const { container } = render(() => <TerminalTab />);
		const checkboxes = container.querySelectorAll("input[type=checkbox]");
		fireEvent.change(checkboxes[2], { target: { checked: true } });
		expect(mockSetShowPromptMarks).toHaveBeenCalledWith(true);
	});

	it("calls setBlockFoldingEnabled when its toggle changes", () => {
		const { container } = render(() => <TerminalTab />);
		const checkboxes = container.querySelectorAll("input[type=checkbox]");
		fireEvent.change(checkboxes[3], { target: { checked: false } });
		expect(mockSetBlockFoldingEnabled).toHaveBeenCalledWith(false);
	});
});
