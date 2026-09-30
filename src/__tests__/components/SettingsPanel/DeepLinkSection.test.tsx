import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

// Deep-linking to `settings-upstream-mcp` opens the MCP tab, which mounts
// `LocalMcpPanel` and `UpstreamMcpPanel` — both call several `rpc()` commands
// on mount, and the test disposes before any of them settle. Stub the whole
// transport instead of each resource: the fetches are incidental to what
// these cases assert (scroll position), and every one has to keep working for
// the panel to render at all.
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	rpc: vi.fn(async () => []),
}));

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		state: { ide: "vscode", font: "JetBrains Mono", defaultFontSize: 12 },
		isAiChatEnabled: () => false,
		isAcpConfigured: () => false,
	},
	IDE_NAMES: { vscode: "VS Code" },
	FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono" },
}));

vi.mock("../../../stores/ui", () => ({
	uiStore: {
		state: { settingsNavWidth: 180 },
		setSettingsNavWidth: vi.fn(),
		persistUIPrefs: vi.fn(),
	},
}));

vi.mock("../../../stores/repositories", () => ({
	repositoriesStore: {
		state: { repositories: {}, repoOrder: [] },
		getAllReposOrdered: () => [],
		getConnectionId: () => undefined,
		setDisplayName: vi.fn(),
	},
}));

import { SettingsPanel } from "../../../components/SettingsPanel/SettingsPanel";
import { SETTINGS_SECTION_UPSTREAM_MCP } from "../../../components/SettingsPanel/sections";

/** Run the frame the scroll effect schedules. */
const nextFrame = () => new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));

describe("SettingsPanel — deep link to a section", () => {
	let scrolled: Element[];

	beforeEach(() => {
		vi.clearAllMocks();
		scrolled = [];
		// jsdom has no layout, so scrollIntoView does not exist.
		Element.prototype.scrollIntoView = function scrollIntoView(this: Element) {
			scrolled.push(this);
		};
	});

	it("scrolls the upstream MCP block into view when asked for it", async () => {
		const { container } = render(() => (
			<SettingsPanel
				visible={true}
				onClose={() => {}}
				initialTab="mcp"
				initialSection={SETTINGS_SECTION_UPSTREAM_MCP}
			/>
		));

		const block = container.querySelector(`#${SETTINGS_SECTION_UPSTREAM_MCP}`);
		expect(block, "upstream MCP block should carry the anchor id").toBeTruthy();

		await nextFrame();
		expect(scrolled).toEqual([block]);
	});

	it("leaves the tab at the top when no section is requested", async () => {
		render(() => <SettingsPanel visible={true} onClose={() => {}} initialTab="mcp" />);
		await nextFrame();
		expect(scrolled).toEqual([]);
	});

	it("does not scroll while the panel is closed", async () => {
		render(() => (
			<SettingsPanel
				visible={false}
				onClose={() => {}}
				initialTab="mcp"
				initialSection={SETTINGS_SECTION_UPSTREAM_MCP}
			/>
		));
		await nextFrame();
		expect(scrolled).toEqual([]);
	});

	it("reopens on the requested tab and section after the user moved elsewhere", async () => {
		const [visible, setVisible] = createSignal(true);
		const { container } = render(() => (
			<SettingsPanel
				visible={visible()}
				onClose={() => {}}
				initialTab="mcp"
				initialSection={SETTINGS_SECTION_UPSTREAM_MCP}
			/>
		));
		await nextFrame();
		const general = Array.from(container.querySelectorAll(".navItem")).find((n) => n.textContent === "General")!;
		fireEvent.click(general);
		expect(container.querySelector(".navItem.active")!.textContent).toBe("General");

		setVisible(false);
		scrolled = [];
		setVisible(true);
		await nextFrame();

		expect(container.querySelector(".navItem.active")!.textContent).toBe("MCP");
		expect(scrolled).toEqual([container.querySelector(`#${SETTINGS_SECTION_UPSTREAM_MCP}`)]);
	});

	it("gives MCP its own direct nav entry, findable without going through Services", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const labels = Array.from(container.querySelectorAll(".navItem")).map((n) => n.textContent);
		expect(labels).toContain("MCP");
	});
});
