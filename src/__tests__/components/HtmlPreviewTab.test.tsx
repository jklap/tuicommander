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
// `listen` too: main's HtmlPreviewTab import graph reaches stores that
// subscribe at module load (e.g. dictation).
vi.mock("../../invoke", () => ({ invoke: mockInvoke, listen: vi.fn().mockResolvedValue(() => {}) }));

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
