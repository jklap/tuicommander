import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

// Story 865: the Developer Tools page folded back into General. TUIC CLI,
// Code Intelligence (MDKB), Default IDE and Custom Launchers render on
// GeneralTab again, with the same desktop/browser gating they had.

const { mockInvoke, mockIsTauri } = vi.hoisted(() => ({ mockInvoke: vi.fn(), mockIsTauri: vi.fn(() => true) }));

vi.mock("../../../invoke", () => ({
	invoke: mockInvoke,
	listen: vi.fn().mockResolvedValue(vi.fn()),
}));

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	isTauri: mockIsTauri,
}));

import { GeneralTab } from "../../../components/SettingsPanel/tabs/GeneralTab";

function headingTexts(container: HTMLElement): string[] {
	return Array.from(container.querySelectorAll("h3")).map((h) => h.childNodes[0]?.textContent?.trim() ?? "");
}

const hasLabel = (container: HTMLElement, text: string) =>
	Array.from(container.querySelectorAll("label")).some((el) => el.textContent === text);

describe("GeneralTab developer tools sections", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockIsTauri.mockReturnValue(true);
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "get_cli_status") {
				return Promise.resolve({
					installed: true,
					path: "/usr/local/bin/tuic",
					version_match: true,
					auto_updatable: true,
					prompt_dismissed: false,
				});
			}
			if (cmd === "mdkb_status") {
				return Promise.resolve({ available: true, connected: true, binaryPath: "/usr/local/bin/mdkb", version: "1.0" });
			}
			return Promise.resolve(undefined);
		});
	});

	afterEach(() => cleanup());

	it("renders TUIC CLI, Code Intelligence, IDE and Custom Launchers on desktop", async () => {
		const { container, findByText } = render(() => <GeneralTab />);

		await findByText("Installed at /usr/local/bin/tuic");
		const headings = headingTexts(container);
		expect(headings).toContain("TUIC CLI");
		expect(headings).toContain("Code Intelligence");
		expect(headings).toContain("IDE");
		expect(headings).toContain("Custom Launchers");
		expect(hasLabel(container, "Default IDE")).toBe(true);
	});

	it("keeps only the IDE picker in a browser", async () => {
		mockIsTauri.mockReturnValue(false);
		const { container } = render(() => <GeneralTab />);

		const headings = headingTexts(container);
		expect(headings).toContain("IDE");
		expect(hasLabel(container, "Default IDE")).toBe(true);
		expect(headings).not.toContain("TUIC CLI");
		expect(headings).not.toContain("Code Intelligence");
		expect(headings).not.toContain("Custom Launchers");
		expect(mockInvoke).not.toHaveBeenCalledWith("get_cli_status");
		expect(mockInvoke).not.toHaveBeenCalledWith("mdkb_status");
	});
});
