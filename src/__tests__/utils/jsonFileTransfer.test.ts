import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { toastsStore } from "../../stores/toasts";
import { exportJsonWithToast, pickJsonImportFile, saveJsonFile } from "../../utils/jsonFileTransfer";
import { mockInvoke } from "../mocks/tauri";

/** setup.ts sets __TAURI_INTERNALS__ globally so every other suite defaults to Tauri mode;
 *  these tests need to flip to browser mode for some cases. */
function setTauriMode(enabled: boolean) {
	if (enabled) {
		(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
	} else {
		delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
	}
}

describe("saveJsonFile — browser mode", () => {
	let createObjectURL: ReturnType<typeof vi.fn>;
	let revokeObjectURL: ReturnType<typeof vi.fn>;
	let click: ReturnType<typeof vi.fn>;
	let anchor: HTMLAnchorElement;

	beforeEach(() => {
		setTauriMode(false);
		createObjectURL = vi.fn(() => "blob:mock-url");
		revokeObjectURL = vi.fn();
		(URL as unknown as { createObjectURL: unknown }).createObjectURL = createObjectURL;
		(URL as unknown as { revokeObjectURL: unknown }).revokeObjectURL = revokeObjectURL;

		click = vi.fn();
		const originalCreateElement = document.createElement.bind(document);
		vi.spyOn(document, "createElement").mockImplementation((tag: string) => {
			const el = originalCreateElement(tag) as HTMLElement;
			if (tag === "a") {
				anchor = el as HTMLAnchorElement;
				anchor.click = click as unknown as () => void;
			}
			return el;
		});
	});

	afterEach(() => {
		vi.restoreAllMocks();
		setTauriMode(true);
	});

	it("falls back to a Blob download and never touches the native dialog", async () => {
		const result = await saveJsonFile("rules-all.json", { a: 1 }, "Export Rules");

		expect(result).toEqual({ saved: true });
		expect(anchor.download).toBe("rules-all.json");
		const blobArg = createObjectURL.mock.calls[0][0] as Blob;
		expect(blobArg.type).toContain("application/json");
		expect(click).toHaveBeenCalledOnce();
		expect(revokeObjectURL).toHaveBeenCalledWith("blob:mock-url");
	});
});

describe("saveJsonFile — Tauri mode", () => {
	beforeEach(() => {
		setTauriMode(true);
		mockInvoke.mockReset().mockResolvedValue(undefined);
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it("opens the native Save dialog and writes through write_external_file", async () => {
		const { save } = await import("@tauri-apps/plugin-dialog");
		vi.mocked(save).mockResolvedValueOnce("/Users/x/Desktop/rules-all.json");

		const result = await saveJsonFile("rules-all.json", { a: 1 }, "Export Rules");

		expect(save).toHaveBeenCalledWith(
			expect.objectContaining({ title: "Export Rules", defaultPath: "rules-all.json" }),
		);
		expect(mockInvoke).toHaveBeenCalledWith("write_external_file", {
			path: "/Users/x/Desktop/rules-all.json",
			content: JSON.stringify({ a: 1 }, null, 2),
		});
		expect(result).toEqual({ saved: true, path: "/Users/x/Desktop/rules-all.json" });
	});

	it("reports a cancelled dialog without writing anything", async () => {
		const { save } = await import("@tauri-apps/plugin-dialog");
		vi.mocked(save).mockResolvedValueOnce(null);

		const result = await saveJsonFile("rules-all.json", { a: 1 }, "Export Rules");

		expect(mockInvoke).not.toHaveBeenCalled();
		expect(result).toEqual({ saved: false });
	});

	it("propagates a write failure", async () => {
		const { save } = await import("@tauri-apps/plugin-dialog");
		vi.mocked(save).mockResolvedValueOnce("/Users/x/Desktop/rules-all.json");
		mockInvoke.mockRejectedValueOnce(new Error("Access denied"));

		await expect(saveJsonFile("rules-all.json", { a: 1 }, "Export Rules")).rejects.toThrow("Access denied");
	});
});

describe("exportJsonWithToast", () => {
	beforeEach(() => {
		setTauriMode(true);
		mockInvoke.mockReset().mockResolvedValue(undefined);
		vi.spyOn(toastsStore, "add").mockImplementation(() => 1);
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it("toasts success with the written path", async () => {
		const { save } = await import("@tauri-apps/plugin-dialog");
		vi.mocked(save).mockResolvedValueOnce("/Users/x/Desktop/rules-all.json");

		await exportJsonWithToast("rules-all.json", { a: 1 }, "Export Rules", "Exported rules");

		expect(toastsStore.add).toHaveBeenCalledWith("Exported rules", "/Users/x/Desktop/rules-all.json", "info");
	});

	it("does not toast when the dialog is cancelled", async () => {
		const { save } = await import("@tauri-apps/plugin-dialog");
		vi.mocked(save).mockResolvedValueOnce(null);

		await exportJsonWithToast("rules-all.json", { a: 1 }, "Export Rules", "Exported rules");

		expect(toastsStore.add).not.toHaveBeenCalled();
	});

	it("toasts an error when the write fails", async () => {
		const { save } = await import("@tauri-apps/plugin-dialog");
		vi.mocked(save).mockResolvedValueOnce("/Users/x/Desktop/rules-all.json");
		mockInvoke.mockRejectedValueOnce(new Error("Access denied"));

		await exportJsonWithToast("rules-all.json", { a: 1 }, "Export Rules", "Exported rules");

		expect(toastsStore.add).toHaveBeenCalledWith("Export failed", "Error: Access denied", "error");
	});
});

describe("pickJsonImportFile", () => {
	let originalCreateElement: typeof document.createElement;
	let input: HTMLInputElement;

	beforeEach(() => {
		originalCreateElement = document.createElement.bind(document);
		vi.spyOn(document, "createElement").mockImplementation((tag: string) => {
			const el = originalCreateElement(tag) as HTMLElement;
			if (tag === "input") input = el as HTMLInputElement;
			return el;
		});
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it("configures a JSON-only file input and resolves with the selected file's text", async () => {
		const promise = pickJsonImportFile();
		expect(input.type).toBe("file");
		expect(input.accept).toBe(".json");

		const file = new File(['{"a":1}'], "export.json", { type: "application/json" });
		Object.defineProperty(input, "files", { value: [file] });
		input.onchange?.(new Event("change"));

		await expect(promise).resolves.toBe('{"a":1}');
	});

	it("resolves null when no file was selected", async () => {
		const promise = pickJsonImportFile();
		Object.defineProperty(input, "files", { value: [] });
		input.onchange?.(new Event("change"));

		await expect(promise).resolves.toBeNull();
	});
});
