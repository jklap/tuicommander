import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";

const { mockWriteClipboard, mockDownloadText } = vi.hoisted(() => ({
	mockWriteClipboard: vi.fn<() => Promise<void>>(() => Promise.resolve()),
	mockDownloadText: vi.fn(),
}));

vi.mock("../../utils/clipboard", () => ({
	writeClipboard: mockWriteClipboard,
}));

vi.mock("../../utils/downloadText", () => ({
	downloadText: mockDownloadText,
}));

import { ChangelogModal } from "../../components/ChangelogModal/ChangelogModal";

describe("ChangelogModal", () => {
	const defaultProps = {
		repoPath: "/repo",
		onClose: vi.fn(),
	};

	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockResolvedValue({ markdown: "## CL\n- x", json: {} });
	});

	afterEach(() => {
		cleanup();
		vi.restoreAllMocks();
	});

	it("shows loading state then renders the markdown", async () => {
		let resolveInvoke!: (value: unknown) => void;
		mockInvoke.mockReturnValueOnce(new Promise((res) => (resolveInvoke = res)));

		const { container } = render(() => <ChangelogModal {...defaultProps} />);
		// Loading state visible before the promise resolves
		expect(container.querySelector(".spinner")).not.toBeNull();

		resolveInvoke({ markdown: "## CL\n- x", json: {} });

		await waitFor(() => {
			expect(container.querySelector(".markdown")).not.toBeNull();
		});
		expect(container.querySelector(".markdown")!.textContent).toContain("## CL");
		expect(container.querySelector(".spinner")).toBeNull();
	});

	it("asks the backend for this repo, and carries no provider or model with it", async () => {
		render(() => <ChangelogModal {...defaultProps} />);
		await waitFor(() => expect(mockInvoke).toHaveBeenCalled());
		// Which model writes the changelog is ego's configuration (criterion 4).
		expect(mockInvoke).toHaveBeenCalledWith("generate_changelog", { repoPath: "/repo" });
	});

	it("renders ego's own sentence when ego is not reachable, never an empty changelog", async () => {
		// Verbatim from `acp::oneshot::ask` when the ACP connection cannot be made.
		const egoSays = "ego could not run this turn: no ego executable is configured";
		mockInvoke.mockRejectedValueOnce(egoSays);

		const { container } = render(() => <ChangelogModal {...defaultProps} />);

		await waitFor(() => {
			expect(container.querySelector(".error")).not.toBeNull();
		});
		expect(container.querySelector(".error")!.textContent).toContain("no ego executable is configured");
		// Not a blank body that reads as "this repo changed nothing".
		expect(container.querySelector(".markdown")).toBeNull();
		expect(container.querySelector(".spinner")).toBeNull();
	});

	it("disables Copy and Save while there is nothing to copy or save", async () => {
		mockInvoke.mockRejectedValueOnce("ego could not run this turn: spawn failed");
		const { container } = render(() => <ChangelogModal {...defaultProps} />);

		await waitFor(() => {
			expect(container.querySelector(".error")).not.toBeNull();
		});
		const buttons = container.querySelectorAll(".btn");
		expect((buttons[0] as HTMLButtonElement).disabled).toBe(true);
		expect((buttons[1] as HTMLButtonElement).disabled).toBe(true);
	});

	it("calls writeClipboard when Copy is clicked", async () => {
		const { container } = render(() => <ChangelogModal {...defaultProps} />);

		await waitFor(() => {
			expect(container.querySelector(".markdown")).not.toBeNull();
		});

		const copyBtn = container.querySelectorAll(".btn")[0] as HTMLButtonElement;
		fireEvent.click(copyBtn);
		expect(mockWriteClipboard).toHaveBeenCalledWith("## CL\n- x");
	});

	it("calls downloadText when Save is clicked", async () => {
		const { container } = render(() => <ChangelogModal {...defaultProps} />);

		await waitFor(() => {
			expect(container.querySelector(".markdown")).not.toBeNull();
		});

		const saveBtn = container.querySelectorAll(".btn")[1] as HTMLButtonElement;
		fireEvent.click(saveBtn);
		expect(mockDownloadText).toHaveBeenCalledWith("CHANGELOG-ai.md", "## CL\n- x");
	});
});
