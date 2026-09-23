import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

const additionalReadableDirsBox = vi.hoisted(() => ({
	dirs: ["~/.claude/plans", "/tmp/other"] as string[],
	setAdditionalReadableDirs: vi.fn(),
}));

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		get state() {
			return { additionalReadableDirs: additionalReadableDirsBox.dirs };
		},
		setAdditionalReadableDirs: additionalReadableDirsBox.setAdditionalReadableDirs,
	},
}));

import { ServicesTab } from "../../../components/SettingsPanel/tabs/ServicesTab";

/** ServicesTab's onMount fires a fire-and-forget `load_config` rpc call (for
 *  the TUIC Tools section's disabled-tools/collapse-tools state) that stays
 *  unresolved past the synchronous test body — flush it so vitest's leak
 *  detector doesn't flag it as dangling. */
async function flushMicrotasks(): Promise<void> {
	await new Promise<void>((resolve) => setImmediate(resolve));
}

describe("ServicesTab — File Access", () => {
	beforeEach(() => {
		additionalReadableDirsBox.dirs = ["~/.claude/plans", "/tmp/other"];
		additionalReadableDirsBox.setAdditionalReadableDirs.mockReset();
	});

	afterEach(async () => {
		vi.clearAllTimers();
		await flushMicrotasks();
	});

	it("renders the File Access section on mount", async () => {
		// `load_config` resolves undefined via the global Tauri mock (no case for
		// it), which the tools-config loader's own try/catch swallows — the File
		// Access section renders unconditionally regardless.
		const { getByText, unmount } = render(() => <ServicesTab />);
		await flushMicrotasks();

		expect(getByText("File Access")).toBeTruthy();
		expect(getByText("Additional Readable Directories")).toBeTruthy();
		unmount();
	});

	it("adding a typed directory calls settingsStore.setAdditionalReadableDirs with the appended list", async () => {
		const { getByPlaceholderText, getByText, unmount } = render(() => <ServicesTab />);
		await flushMicrotasks();

		const input = getByPlaceholderText("e.g. ~/.claude/plans") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "/Users/dev/notes" } });
		fireEvent.click(getByText("Add"));

		expect(additionalReadableDirsBox.setAdditionalReadableDirs).toHaveBeenCalledWith([
			"~/.claude/plans",
			"/tmp/other",
			"/Users/dev/notes",
		]);
		unmount();
	});

	it("removing a directory calls it with the list minus that entry", async () => {
		const { getAllByText, unmount } = render(() => <ServicesTab />);
		await flushMicrotasks();

		const removeButtons = getAllByText("Remove");
		fireEvent.click(removeButtons[0]);

		expect(additionalReadableDirsBox.setAdditionalReadableDirs).toHaveBeenCalledWith(["/tmp/other"]);
		unmount();
	});

	it("refuses to add a blank/whitespace-only entry", async () => {
		const { getByPlaceholderText, getByText, unmount } = render(() => <ServicesTab />);
		await flushMicrotasks();

		const input = getByPlaceholderText("e.g. ~/.claude/plans") as HTMLInputElement;
		const addButton = getByText("Add") as HTMLButtonElement;

		fireEvent.input(input, { target: { value: "   " } });
		expect(addButton.disabled).toBe(true);

		fireEvent.click(addButton);
		expect(additionalReadableDirsBox.setAdditionalReadableDirs).not.toHaveBeenCalled();
		unmount();
	});

	it("refuses a relative path with a visible error, and clears it on next input", async () => {
		const { getByPlaceholderText, getByText, queryByText, unmount } = render(() => <ServicesTab />);
		await flushMicrotasks();

		const input = getByPlaceholderText("e.g. ~/.claude/plans") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "relative/dir" } });
		fireEvent.click(getByText("Add"));

		expect(additionalReadableDirsBox.setAdditionalReadableDirs).not.toHaveBeenCalled();
		expect(queryByText(/Must be an absolute path/)).not.toBeNull();

		// Typing again clears the stale error instead of leaving it stuck.
		fireEvent.input(input, { target: { value: "~/notes" } });
		expect(queryByText(/Must be an absolute path/)).toBeNull();
		unmount();
	});

	it("refuses an entry containing '..' with a visible error", async () => {
		const { getByPlaceholderText, getByText, queryByText, unmount } = render(() => <ServicesTab />);
		await flushMicrotasks();

		const input = getByPlaceholderText("e.g. ~/.claude/plans") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "~/../etc" } });
		fireEvent.click(getByText("Add"));

		expect(additionalReadableDirsBox.setAdditionalReadableDirs).not.toHaveBeenCalled();
		expect(queryByText(/Must be an absolute path/)).not.toBeNull();
		unmount();
	});

	it("accepts a bare ~/ entry (the documented placeholder form)", async () => {
		const { getByPlaceholderText, getByText, unmount } = render(() => <ServicesTab />);
		await flushMicrotasks();

		const input = getByPlaceholderText("e.g. ~/.claude/plans") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "~/notes" } });
		fireEvent.click(getByText("Add"));

		expect(additionalReadableDirsBox.setAdditionalReadableDirs).toHaveBeenCalledWith([
			"~/.claude/plans",
			"/tmp/other",
			"~/notes",
		]);
		unmount();
	});
});
