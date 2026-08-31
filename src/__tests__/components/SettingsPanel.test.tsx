import { beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { fireEvent, render } from "@solidjs/testing-library";

vi.mock("../../stores/settings", () => ({
	settingsStore: {
		state: {
			ide: "vscode",
			font: "JetBrains Mono",
			defaultFontSize: 12,
			fontWeight: 400,
			cursorStyle: "bar",
			theme: "vscode-dark",
			confirmBeforeQuit: true,
			confirmBeforeClosingTab: true,
			copyOnSelect: true,
			osc52Clipboard: true,
			showLastPrompt: true,
			linkActivation: "click",
			doubleClickAction: "smart",
			wordSelectionMode: "characters",
			wordSeparators: " \"'`(){}[]<>|;:,.!?@#$%^&*~=+/\\",
			wordSelectionRegex: "",
			smartSelectionRules: [],
			blockTimestampMode: "modifier",
			showBlockMarks: true,
			showPromptMarks: true,
			blockFoldingEnabled: true,
			shell: "",
		},
		setShell: vi.fn(),
		setIde: vi.fn(),
		setFont: vi.fn(),
		setDefaultFontSize: vi.fn(),
		setFontWeight: vi.fn(),
		setCursorStyle: vi.fn(),
		setConfirmBeforeQuit: vi.fn(),
		setConfirmBeforeClosingTab: vi.fn(),
		setCopyOnSelect: vi.fn(),
		setOsc52Clipboard: vi.fn(),
		setShowLastPrompt: vi.fn(),
		setLinkActivation: vi.fn(),
		setDoubleClickAction: vi.fn(),
		setWordSelectionMode: vi.fn(),
		setWordSeparators: vi.fn(),
		setWordSelectionRegex: vi.fn(),
		setSmartSelectionRules: vi.fn(),
		setBlockTimestampMode: vi.fn(),
		setShowBlockMarks: vi.fn(),
		setShowPromptMarks: vi.fn(),
		setBlockFoldingEnabled: vi.fn(),
		isAiChatEnabled: vi.fn().mockReturnValue(false),
	},
	IDE_NAMES: { vscode: "VS Code", cursor: "Cursor" },
	FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono", "Fira Code": "Fira Code" },
}));

vi.mock("../../stores/notifications", () => ({
	notificationsStore: {
		state: {
			isAvailable: true,
			config: {
				enabled: true,
				volume: 0.5,
				sounds: {
					question: true,
					error: true,
					completion: true,
					warning: true,
				},
			},
		},
		setEnabled: vi.fn(),
		setVolume: vi.fn(),
		setSoundEnabled: vi.fn(),
		testSound: vi.fn(),
		reset: vi.fn(),
	},
}));

vi.mock("../../stores/ui", () => ({
	uiStore: {
		state: {
			settingsNavWidth: 180,
		},
		setSettingsNavWidth: vi.fn(),
	},
}));

vi.mock("../../stores/repositories", () => {
	const repositories = {
		"/repo/alpha": { path: "/repo/alpha", displayName: "Alpha" },
		"/repo/beta": { path: "/repo/beta", displayName: "Beta" },
	};
	const repoOrder = ["/repo/alpha", "/repo/beta"];
	return {
		repositoriesStore: {
			state: { repositories, repoOrder },
			setDisplayName: vi.fn(),
			getGroupForRepo: vi.fn(() => undefined),
			getAllReposOrdered: vi.fn(() => repoOrder.map((p) => repositories[p as keyof typeof repositories])),
		},
	};
});

vi.mock("../../stores/repoSettings", () => ({
	repoSettingsStore: {
		get: vi.fn(() => undefined),
		getOrCreate: vi.fn().mockReturnValue({
			path: "/repo/alpha",
			displayName: "Alpha",
			baseBranch: "automatic",
			copyIgnoredFiles: false,
			copyUntrackedFiles: false,
			setupScript: "",
			runScript: "",
			color: "",
		}),
		update: vi.fn(),
		reset: vi.fn(),
	},
}));

import { SettingsPanel } from "../../components/SettingsPanel/SettingsPanel";

describe("SettingsPanel", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("does not render when visible=false", () => {
		const { container } = render(() => <SettingsPanel visible={false} onClose={() => {}} />);
		const overlay = container.querySelector(".overlay");
		expect(overlay).toBeNull();
	});

	it("renders when visible=true", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const overlay = container.querySelector(".overlay");
		expect(overlay).not.toBeNull();
	});

	it("shows Settings header", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const heading = container.querySelector(".header h2");
		expect(heading).not.toBeNull();
		expect(heading!.textContent).toBe("Settings");
	});

	it("shows nav items (General, Notifications)", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const navItems = container.querySelectorAll(".navItem");
		const labels = Array.from(navItems).map((n) => n.textContent);
		expect(labels).toContain("General");
		expect(labels).toContain("Appearance");
		expect(labels).toContain("Terminal");
		expect(labels).toContain("Notifications");
		expect(labels).toContain("Agents");
		expect(labels).not.toContain("Groups");
	});

	it("shows the Shell/Rendering/Behavior/Blocks/Session Restore groups when the Terminal nav item is active", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const navItems = container.querySelectorAll(".navItem");
		const terminalItem = Array.from(navItems).find((n) => n.textContent === "Terminal")!;
		fireEvent.click(terminalItem);

		const headings = Array.from(container.querySelectorAll(".section h3")).map((h) => h.textContent);
		expect(headings).toEqual(["Shell", "Rendering", "Behavior", "Blocks", "Shell Integration", "Session Restore"]);

		const toggleLabels = Array.from(container.querySelectorAll(".toggle span")).map((n) => n.textContent);
		expect(toggleLabels).toContain("Copy on select");
		expect(toggleLabels).toContain("Allow OSC 52 clipboard writes");
		// blockTimestampMode is a 3-way SettingSelect (off/modifier/always), not a
		// SettingToggle, so it isn't among the `.toggle span` labels above.
		expect(container.textContent).toContain("Show block timestamps");
		expect(toggleLabels).toContain("Show block marks");
		expect(toggleLabels).toContain("Show prompt marks");
		expect(toggleLabels).toContain("Enable block folding");
		expect(toggleLabels).toContain("Restore open terminals on launch");
		expect(toggleLabels).toContain("Save terminal scrollback");
	});

	it("shows the Smart Selection nav item and its content when active", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const navItems = container.querySelectorAll(".navItem");
		const selectionItem = Array.from(navItems).find((n) => n.textContent === "Smart Selection")!;
		expect(selectionItem).toBeTruthy();
		fireEvent.click(selectionItem);

		const headings = Array.from(container.querySelectorAll(".section h3")).map((h) => h.textContent);
		expect(headings).toEqual(["Behavior", "Word Boundaries", "Smart Selection Rules"]);
	});

	it("close button calls onClose", () => {
		const onClose = vi.fn();
		const { container } = render(() => <SettingsPanel visible={true} onClose={onClose} />);
		const closeBtn = container.querySelector(".close")!;
		fireEvent.click(closeBtn);
		expect(onClose).toHaveBeenCalledOnce();
	});

	it("overlay click calls onClose", () => {
		const onClose = vi.fn();
		const { container } = render(() => <SettingsPanel visible={true} onClose={onClose} />);
		const overlay = container.querySelector(".overlay")!;
		fireEvent.click(overlay);
		expect(onClose).toHaveBeenCalledOnce();
	});

	it("switching nav items shows correct content", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);

		// Default is General (Git Integration moved to GitHub tab)
		const headings = container.querySelectorAll(".section h3");
		expect(headings.length).toBeGreaterThanOrEqual(5);
		// Use childNodes[0] to get heading text without tooltip content
		const headingTexts = Array.from(headings).map((h) => h.childNodes[0]?.textContent?.trim() ?? "");
		expect(headingTexts).toContain("General");
		expect(headingTexts).toContain("Confirmations");
		expect(headingTexts).toContain("Power Management");
		expect(headingTexts).toContain("Updates");

		// Click Notifications nav item
		const navItems = container.querySelectorAll(".navItem");
		const notificationsItem = Array.from(navItems).find((n) => n.textContent === "Notifications")!;
		fireEvent.click(notificationsItem);
		const sectionTitle = container.querySelector(".section h3");
		expect(sectionTitle!.textContent).toBe("Notification Settings");
	});

	it("shows repos as nav items in the sidebar", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const repoItems = container.querySelectorAll(".navItemRepo");
		const labels = Array.from(repoItems).map((n) => n.textContent);
		expect(labels).toContain("Alpha");
		expect(labels).toContain("Beta");
	});

	it("shows REPOSITORIES section label above repo items", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const label = container.querySelector(".navLabel");
		expect(label).not.toBeNull();
		expect(label!.textContent).toBe("REPOSITORIES");
	});

	it("opens on General when no context given", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const activeItem = container.querySelector(".navItem.active");
		expect(activeItem!.textContent).toBe("General");
	});

	it("opens directly on repo nav item when context is repo", () => {
		const { container } = render(() => (
			<SettingsPanel visible={true} onClose={() => {}} context={{ kind: "repo", repoPath: "/repo/alpha" }} />
		));
		const activeItem = container.querySelector(".navItem.active");
		expect(activeItem!.classList.contains("navItemRepo")).toBe(true);
		expect(activeItem!.textContent).toBe("Alpha");
	});

	it("shows repo settings content when repo nav item is active", () => {
		const { container } = render(() => (
			<SettingsPanel visible={true} onClose={() => {}} context={{ kind: "repo", repoPath: "/repo/alpha" }} />
		));
		// RepoWorktreeTab has a h3 "Repository"
		const h3 = container.querySelector(".section h3");
		expect(h3!.textContent).toBe("Repository");
	});

	it("shows Reset to Defaults button only when repo nav item is active", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		// Global context → no Reset button
		expect(container.querySelector(".footerReset")).toBeNull();
	});

	it("shows Reset to Defaults when repo nav item is active", () => {
		const { container } = render(() => (
			<SettingsPanel visible={true} onClose={() => {}} context={{ kind: "repo", repoPath: "/repo/alpha" }} />
		));
		expect(container.querySelector(".footerReset")).not.toBeNull();
	});
});
