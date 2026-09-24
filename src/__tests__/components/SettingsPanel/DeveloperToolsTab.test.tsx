import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

// Story 858: TUIC CLI, Code Intelligence (MDKB), Default IDE and Custom
// Launchers moved off GeneralTab onto a new DeveloperToolsTab, preserving
// desktop/browser gating exactly. These tests prove the fields landed on the
// new page and are gone from GeneralTab.

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock("../../../invoke", () => ({
	invoke: mockInvoke,
	listen: vi.fn().mockResolvedValue(vi.fn()),
}));

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { DeveloperToolsTab } from "../../../components/SettingsPanel/tabs/DeveloperToolsTab";
import { GeneralTab } from "../../../components/SettingsPanel/tabs/GeneralTab";

function headingTexts(container: HTMLElement): string[] {
	return Array.from(container.querySelectorAll("h3")).map((h) => h.childNodes[0]?.textContent?.trim() ?? "");
}

describe("DeveloperToolsTab placement", () => {
	beforeEach(() => {
		vi.clearAllMocks();
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

	it("renders TUIC CLI, Code Intelligence, IDE and Custom Launchers", async () => {
		const { container, findByText } = render(() => <DeveloperToolsTab />);

		await findByText("Default IDE");
		const headings = headingTexts(container);
		expect(headings).toContain("TUIC CLI");
		expect(headings).toContain("Code Intelligence");
		expect(headings).toContain("IDE");
		expect(headings).toContain("Custom Launchers");
	});

	it("does not render any of those sections on GeneralTab", async () => {
		const { container } = render(() => <GeneralTab />);
		const headings = headingTexts(container);
		expect(headings).not.toContain("TUIC CLI");
		expect(headings).not.toContain("Code Intelligence");
		expect(headings).not.toContain("Custom Launchers");
		expect(Array.from(container.querySelectorAll("label")).some((el) => el.textContent === "Default IDE")).toBe(false);
	});
});
