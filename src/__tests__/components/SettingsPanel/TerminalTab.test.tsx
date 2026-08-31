import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import { fireEvent, render } from "@solidjs/testing-library";
import { resetPlatformCache } from "../../../platform";

const {
	mockSetBlockTimestampMode,
	mockSetShowBlockMarks,
	mockSetShowPromptMarks,
	mockSetBlockFoldingEnabled,
	mockSetFont,
	mockSetDefaultFontSize,
	mockSetFontWeight,
	mockSetCursorStyle,
	mockSetCopyOnSelect,
	mockSetOsc52Clipboard,
	mockSetShowLastPrompt,
	mockSetLinkActivation,
	mockSetRestoreShellTerminals,
	mockSetRestoreScrollback,
	mockSetRestoreScrollbackLines,
	mockSetShell,
	mockWriteClipboard,
} = vi.hoisted(() => ({
	mockSetBlockTimestampMode: vi.fn(),
	mockSetShowBlockMarks: vi.fn(),
	mockSetShowPromptMarks: vi.fn(),
	mockSetBlockFoldingEnabled: vi.fn(),
	mockSetFont: vi.fn(),
	mockSetDefaultFontSize: vi.fn(),
	mockSetFontWeight: vi.fn(),
	mockSetCursorStyle: vi.fn(),
	mockSetCopyOnSelect: vi.fn(),
	mockSetOsc52Clipboard: vi.fn(),
	mockSetShowLastPrompt: vi.fn(),
	mockSetLinkActivation: vi.fn(),
	mockSetRestoreShellTerminals: vi.fn(),
	mockSetRestoreScrollback: vi.fn(),
	mockSetRestoreScrollbackLines: vi.fn(),
	mockSetShell: vi.fn(),
	mockWriteClipboard: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../../utils/clipboard", () => ({ writeClipboard: mockWriteClipboard }));

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		state: {
			font: "JetBrains Mono",
			defaultFontSize: 14,
			fontWeight: 400,
			cursorStyle: "bar",
			theme: "vscode-dark",
			copyOnSelect: true,
			osc52Clipboard: true,
			showLastPrompt: false,
			linkActivation: "click",
			blockTimestampMode: "modifier",
			showBlockMarks: true,
			showPromptMarks: false,
			blockFoldingEnabled: true,
			restoreShellTerminals: true,
			restoreScrollback: false,
			restoreScrollbackLines: 1000,
			shell: "",
		},
		setShell: mockSetShell,
		setFont: mockSetFont,
		setDefaultFontSize: mockSetDefaultFontSize,
		setFontWeight: mockSetFontWeight,
		setCursorStyle: mockSetCursorStyle,
		setCopyOnSelect: mockSetCopyOnSelect,
		setOsc52Clipboard: mockSetOsc52Clipboard,
		setShowLastPrompt: mockSetShowLastPrompt,
		setLinkActivation: mockSetLinkActivation,
		setBlockTimestampMode: mockSetBlockTimestampMode,
		setShowBlockMarks: mockSetShowBlockMarks,
		setShowPromptMarks: mockSetShowPromptMarks,
		setBlockFoldingEnabled: mockSetBlockFoldingEnabled,
		setRestoreShellTerminals: mockSetRestoreShellTerminals,
		setRestoreScrollback: mockSetRestoreScrollback,
		setRestoreScrollbackLines: mockSetRestoreScrollbackLines,
	},
	FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono", "Fira Code": "Fira Code" },
}));

import { TerminalTab } from "../../../components/SettingsPanel/tabs/TerminalTab";

describe("TerminalTab", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("renders the Shell, Rendering, Behavior, Blocks, Shell Integration, and Session Restore headings in order", () => {
		const { container } = render(() => <TerminalTab />);
		const headings = Array.from(container.querySelectorAll("h3")).map((h) => h.textContent);
		expect(headings).toEqual(["Shell", "Rendering", "Behavior", "Blocks", "Shell Integration", "Session Restore"]);
	});

	it("calls setShell when the Shell field changes", () => {
		const { getByPlaceholderText } = render(() => <TerminalTab />);
		const shellInput = getByPlaceholderText("Default shell") as HTMLInputElement;
		fireEvent.input(shellInput, { target: { value: "/bin/zsh" } });
		expect(mockSetShell).toHaveBeenCalledWith("/bin/zsh");
	});

	it("shows all eight toggles with the correct checked state", () => {
		// showBlockTimestamps was a checkbox; it's now the blockTimestampMode
		// <select> (see the dedicated tests below), so the count drops from 9.
		const { container } = render(() => <TerminalTab />);
		const checkboxes = Array.from(container.querySelectorAll("input[type=checkbox]")) as HTMLInputElement[];
		expect(checkboxes).toHaveLength(8);
		expect(checkboxes.map((cb) => cb.checked)).toEqual([true, true, false, true, false, true, true, false]);
	});

	it("calls setCopyOnSelect when its toggle changes", () => {
		const { container } = render(() => <TerminalTab />);
		const checkboxes = container.querySelectorAll("input[type=checkbox]");
		fireEvent.change(checkboxes[0], { target: { checked: false } });
		expect(mockSetCopyOnSelect).toHaveBeenCalledWith(false);
	});

	it("calls setOsc52Clipboard when its toggle changes", () => {
		const { container } = render(() => <TerminalTab />);
		const checkboxes = container.querySelectorAll("input[type=checkbox]");
		fireEvent.change(checkboxes[1], { target: { checked: false } });
		expect(mockSetOsc52Clipboard).toHaveBeenCalledWith(false);
	});

	it("calls setShowLastPrompt when its toggle changes", () => {
		const { container } = render(() => <TerminalTab />);
		const checkboxes = container.querySelectorAll("input[type=checkbox]");
		fireEvent.change(checkboxes[2], { target: { checked: true } });
		expect(mockSetShowLastPrompt).toHaveBeenCalledWith(true);
	});

	it('labels the showLastPrompt toggle "Show agent context bar"', () => {
		// Every other assertion in this file is index/checked-state based and
		// would pass unchanged if this literal (not run through t(), unlike its
		// neighbors) silently reverted during a merge — see efab3cbe, which
		// moved this control from GeneralTab into this file, and 02e9629d
		// (main), which renamed it from "Show PTY prompt bar" here.
		const { container } = render(() => <TerminalTab />);
		expect(container.textContent).toContain("Show agent context bar");
	});

	it("shows the block timestamp mode select with the current value and its three options", () => {
		const { container } = render(() => <TerminalTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const modeSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "always"))!;
		expect(modeSelect.value).toBe("modifier");
		expect(Array.from(modeSelect.options).map((o) => o.value)).toEqual(["off", "modifier", "always"]);
	});

	it("calls setBlockTimestampMode when the block timestamp mode select changes", () => {
		const { container } = render(() => <TerminalTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const modeSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "always"))!;
		fireEvent.change(modeSelect, { target: { value: "always" } });
		expect(mockSetBlockTimestampMode).toHaveBeenCalledWith("always");
	});

	it("calls setShowBlockMarks when its toggle changes", () => {
		const { container } = render(() => <TerminalTab />);
		const checkboxes = container.querySelectorAll("input[type=checkbox]");
		fireEvent.change(checkboxes[3], { target: { checked: false } });
		expect(mockSetShowBlockMarks).toHaveBeenCalledWith(false);
	});

	it("calls setShowPromptMarks when its toggle changes", () => {
		const { container } = render(() => <TerminalTab />);
		const checkboxes = container.querySelectorAll("input[type=checkbox]");
		fireEvent.change(checkboxes[4], { target: { checked: true } });
		expect(mockSetShowPromptMarks).toHaveBeenCalledWith(true);
	});

	it("calls setBlockFoldingEnabled when its toggle changes", () => {
		const { container } = render(() => <TerminalTab />);
		const checkboxes = container.querySelectorAll("input[type=checkbox]");
		fireEvent.change(checkboxes[5], { target: { checked: false } });
		expect(mockSetBlockFoldingEnabled).toHaveBeenCalledWith(false);
	});

	it("calls setRestoreShellTerminals when its toggle changes", () => {
		const { container } = render(() => <TerminalTab />);
		const checkboxes = container.querySelectorAll("input[type=checkbox]");
		fireEvent.change(checkboxes[6], { target: { checked: false } });
		expect(mockSetRestoreShellTerminals).toHaveBeenCalledWith(false);
	});

	it("calls setRestoreScrollback when its toggle changes", () => {
		const { container } = render(() => <TerminalTab />);
		const checkboxes = container.querySelectorAll("input[type=checkbox]");
		fireEvent.change(checkboxes[7], { target: { checked: true } });
		expect(mockSetRestoreScrollback).toHaveBeenCalledWith(true);
	});

	it("calls setRestoreScrollbackLines when the slider changes", () => {
		const { container } = render(() => <TerminalTab />);
		const sliders = Array.from(container.querySelectorAll("input[type=range]")) as HTMLInputElement[];
		const scrollbackSlider = sliders[sliders.length - 1];
		fireEvent.input(scrollbackSlider, { target: { value: "2000" } });
		expect(mockSetRestoreScrollbackLines).toHaveBeenCalledWith(2000);
	});

	it('shows a "Clear saved scrollback" button', () => {
		const { container } = render(() => <TerminalTab />);
		expect(container.textContent).toContain("Clear saved scrollback");
	});

	describe("shell integration snippets", () => {
		it("shows the bash and fish snippets", () => {
			const { container } = render(() => <TerminalTab />);
			expect(container.textContent).toContain('[ -n "$TUIC_SHELL_INTEGRATION" ] && source "$TUIC_SHELL_INTEGRATION"');
			expect(container.textContent).toContain("if set -q TUIC_SHELL_INTEGRATION; source $TUIC_SHELL_INTEGRATION; end");
		});

		it("copies the bash snippet and shows 'Copied!' feedback", async () => {
			const { getAllByText, findByText, unmount } = render(() => <TerminalTab />);
			fireEvent.click(getAllByText("Copy")[0]);
			expect(mockWriteClipboard).toHaveBeenCalledWith(
				'[ -n "$TUIC_SHELL_INTEGRATION" ] && source "$TUIC_SHELL_INTEGRATION"',
			);
			await findByText("Copied!");
			// The "Copied!" reset is a real setTimeout(2000) — unmount (which runs the
			// component's onCleanup) rather than let it dangle past the test.
			unmount();
		});

		it("copies the fish snippet independently of the bash one", async () => {
			const { getAllByText, findAllByText, unmount } = render(() => <TerminalTab />);
			fireEvent.click(getAllByText("Copy")[1]);
			expect(mockWriteClipboard).toHaveBeenCalledWith(
				"if set -q TUIC_SHELL_INTEGRATION; source $TUIC_SHELL_INTEGRATION; end",
			);
			const copiedLabels = await findAllByText("Copied!");
			expect(copiedLabels).toHaveLength(1);
			unmount();
		});
	});

	it("shows the font family select with the current value", () => {
		const { container } = render(() => <TerminalTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const fontSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "JetBrains Mono"))!;
		expect(fontSelect.value).toBe("JetBrains Mono");
	});

	it("calls setFont when the font select changes", () => {
		const { container } = render(() => <TerminalTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const fontSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "Fira Code"))!;
		fireEvent.change(fontSelect, { target: { value: "Fira Code" } });
		expect(mockSetFont).toHaveBeenCalledWith("Fira Code");
	});

	it("calls setCursorStyle when the cursor style select changes", () => {
		const { container } = render(() => <TerminalTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const cursorSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "block"))!;
		fireEvent.change(cursorSelect, { target: { value: "block" } });
		expect(mockSetCursorStyle).toHaveBeenCalledWith("block");
	});

	it("shows the link activation select with the current value and its three options", () => {
		const { container } = render(() => <TerminalTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const linkSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "modifier"))!;
		expect(linkSelect.value).toBe("click");
		expect(Array.from(linkSelect.options).map((o) => o.value)).toEqual(["click", "modifier", "never"]);
	});

	it("calls setLinkActivation when the link activation select changes", () => {
		const { container } = render(() => <TerminalTab />);
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		const linkSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "modifier"))!;
		fireEvent.change(linkSelect, { target: { value: "modifier" } });
		expect(mockSetLinkActivation).toHaveBeenCalledWith("modifier");
	});

	describe("modifier-symbol-dependent label/hint text", () => {
		const originalPlatform = Object.getOwnPropertyDescriptor(navigator, "platform");

		afterEach(() => {
			if (originalPlatform) Object.defineProperty(navigator, "platform", originalPlatform);
			resetPlatformCache();
		});

		function setPlatform(value: string) {
			Object.defineProperty(navigator, "platform", { value, configurable: true });
			// The component reads isMacOS(), which memoizes detectPlatform() —
			// without this, whichever platform an earlier test in this file (or an
			// earlier file in the same worker) set first sticks for every render.
			resetPlatformCache();
		}

		it("labels the modifier option ⌘Click and says Cmd in the hint on macOS", () => {
			setPlatform("MacIntel");
			const { container, getByText } = render(() => <TerminalTab />);
			const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
			const linkSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "modifier"))!;
			const modifierOption = Array.from(linkSelect.options).find((o) => o.value === "modifier")!;

			expect(modifierOption.textContent).toBe("⌘Click");
			expect(getByText(/Cmd is held/)).toBeTruthy();
		});

		it("labels the modifier option Ctrl+Click and says Ctrl in the hint off macOS", () => {
			setPlatform("Win32");
			const { container, getByText } = render(() => <TerminalTab />);
			const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
			const linkSelect = selects.find((s) => Array.from(s.options).some((o) => o.value === "modifier"))!;
			const modifierOption = Array.from(linkSelect.options).find((o) => o.value === "modifier")!;

			expect(modifierOption.textContent).toBe("Ctrl+Click");
			expect(getByText(/Ctrl is held/)).toBeTruthy();
		});
	});
});
