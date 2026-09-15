// @vitest-environment jsdom

import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockReadFile, mockInvoke } = vi.hoisted(() => ({
	mockReadFile: vi.fn(),
	mockInvoke: vi.fn(),
}));

vi.mock("../../hooks/useRepository", () => ({
	useRepository: () => ({ readFile: mockReadFile }),
}));
vi.mock("../../invoke", () => ({ invoke: mockInvoke }));

import { HtmlPreviewTab } from "../../components/HtmlPreviewTab/HtmlPreviewTab";
import { appLogger } from "../../stores/appLogger";
import type { HtmlPreviewTab as HtmlPreviewTabData } from "../../stores/mdTabs";
import { mdTabsStore } from "../../stores/mdTabs";

function textTab(overrides: Partial<HtmlPreviewTabData> = {}): HtmlPreviewTabData {
	return {
		id: "tab1",
		type: "html-preview",
		title: "notes.txt",
		repoPath: "",
		filePath: "/etc/notes.txt",
		fileName: "notes.txt",
		fsRoot: undefined,
		...overrides,
	};
}

describe("HtmlPreviewTab", () => {
	let unmount: (() => void) | undefined;

	beforeEach(() => {
		mockReadFile.mockReset();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		vi.spyOn(appLogger, "error").mockImplementation(() => {});
		vi.spyOn(appLogger, "debug").mockImplementation(() => {});
		mdTabsStore.clearAll();
	});

	afterEach(() => {
		unmount?.();
		unmount = undefined;
		mdTabsStore.clearAll();
		vi.restoreAllMocks();
	});

	it("renders plain text content for a .txt file read via read_external_file", async () => {
		mockInvoke.mockResolvedValue("hello world");
		const result = render(() => <HtmlPreviewTab tab={textTab()} />);
		unmount = result.unmount;

		await waitFor(() => expect(result.container.textContent).toContain("hello world"));
		expect(mockInvoke).toHaveBeenCalledWith("read_external_file", { path: "/etc/notes.txt" });
	});

	// HTTP-transport-only: this 403 can only come back from `invoke("read_external_file", ...)`
	// on an absolute path, which is unreachable on native Tauri.
	it("shows a friendly message and logs quietly for an HTTP read-gate 403", async () => {
		mockInvoke.mockRejectedValue(
			new Error(
				'RPC read_external_file failed: 403 {"error":"Access denied: path must be within a registered repository or an allowed directory"}',
			),
		);
		const result = render(() => <HtmlPreviewTab tab={textTab()} />);
		unmount = result.unmount;

		await waitFor(() => expect(result.container.textContent).toContain("registered repositories"));
		expect(appLogger.error).not.toHaveBeenCalled();
		expect(appLogger.debug).toHaveBeenCalledWith(
			"app",
			"File preview: read denied by the HTTP read gate",
			expect.anything(),
		);
	});

	it("still logs at error level and renders the raw message for a real error (regression guard)", async () => {
		mockInvoke.mockRejectedValue(new Error("permission denied"));
		const result = render(() => <HtmlPreviewTab tab={textTab()} />);
		unmount = result.unmount;

		await waitFor(() => expect(result.container.textContent).toContain("permission denied"));
		expect(appLogger.error).toHaveBeenCalledWith(
			"app",
			"File preview: read failed",
			expect.objectContaining({ error: "permission denied" }),
		);
	});
});
