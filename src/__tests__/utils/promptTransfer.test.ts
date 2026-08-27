import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PromptExportFile } from "../../utils/promptExport";
import { PROMPT_EXPORT_KIND, PROMPT_EXPORT_SCHEMA_VERSION } from "../../utils/promptExport";
import { downloadPromptExport, pickPromptImportFile } from "../../utils/promptTransfer";

function makeExportFile(): PromptExportFile {
	return {
		kind: PROMPT_EXPORT_KIND,
		schemaVersion: PROMPT_EXPORT_SCHEMA_VERSION,
		exportedAt: 1,
		scope: "all",
		prompts: [],
	};
}

describe("downloadPromptExport", () => {
	let createObjectURL: ReturnType<typeof vi.fn>;
	let revokeObjectURL: ReturnType<typeof vi.fn>;
	let click: ReturnType<typeof vi.fn>;
	let anchor: HTMLAnchorElement;

	beforeEach(() => {
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
	});

	it("downloads the export file as JSON with the given filename", () => {
		downloadPromptExport(makeExportFile(), "smart-prompts-all.json");

		const blobArg = createObjectURL.mock.calls[0][0] as Blob;
		expect(blobArg).toBeInstanceOf(Blob);
		expect(blobArg.type).toContain("application/json");
		expect(anchor.download).toBe("smart-prompts-all.json");
		expect(anchor.getAttribute("href")).toBe("blob:mock-url");
		expect(click).toHaveBeenCalledOnce();
		expect(revokeObjectURL).toHaveBeenCalledWith("blob:mock-url");
	});
});

describe("pickPromptImportFile", () => {
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
		const promise = pickPromptImportFile();
		expect(input.type).toBe("file");
		expect(input.accept).toBe(".json");

		const file = new File(['{"a":1}'], "export.json", { type: "application/json" });
		Object.defineProperty(input, "files", { value: [file] });
		input.onchange?.(new Event("change"));

		await expect(promise).resolves.toBe('{"a":1}');
	});

	it("resolves null when no file was selected", async () => {
		const promise = pickPromptImportFile();
		Object.defineProperty(input, "files", { value: [] });
		input.onchange?.(new Event("change"));

		await expect(promise).resolves.toBeNull();
	});
});
