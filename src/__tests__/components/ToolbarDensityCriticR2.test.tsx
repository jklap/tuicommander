import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../../components/IdeLauncher", () => ({ IdeLauncher: () => <div /> }));
vi.mock("../../components/PrDetailPopover/PrDetailPopover", () => ({ PrDetailPopover: () => <div /> }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/window", () => ({
	getCurrentWindow: vi.fn(() => ({ listen: vi.fn().mockResolvedValue(vi.fn()), setTitle: vi.fn() })),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn().mockResolvedValue(undefined) }));

import { Toolbar } from "../../components/Toolbar/Toolbar";
import { uiStore } from "../../stores/ui";

afterEach(cleanup);

describe("Toolbar density marker after removing the auto outline (critic 1351)", () => {
	it("still tells auto from manual: only manual modes carry the active class, and data-mode tracks the letter — catches auto and manual rendering identically", () => {
		const { container } = render(() => <Toolbar />);
		const toggle = container.querySelector("[data-testid='sidebar-density-toggle']") as HTMLElement;
		const label = () => container.querySelector("[data-testid='sidebar-density-label']")?.textContent;
		const seen: Array<[string | null, string | undefined, boolean]> = [];
		for (let i = 0; i < 3; i++) {
			seen.push([toggle.getAttribute("data-mode"), label(), toggle.className.includes("filterToggleActive")]);
			fireEvent.click(toggle);
		}
		expect(uiStore.state.sidebarDensityMode).toBe(seen[0][0]);
		expect(seen.map(([m, l]) => `${m}:${l}`).sort()).toEqual(["auto:A", "compact:C", "rich:R"]);
		for (const [mode, , active] of seen) expect(active).toBe(mode !== "auto");
	});
});
