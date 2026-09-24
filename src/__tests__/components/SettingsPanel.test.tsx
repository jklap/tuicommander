import { beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { fireEvent, render } from "@solidjs/testing-library";

vi.mock("../../stores/settings", () => ({
	settingsStore: {
		state: {
			ide: "vscode",
			font: "JetBrains Mono",
			defaultFontSize: 12,
			confirmBeforeQuit: true,
			confirmBeforeClosingTab: true,
		},
		setIde: vi.fn(),
		setFont: vi.fn(),
		setConfirmBeforeQuit: vi.fn(),
		setConfirmBeforeClosingTab: vi.fn(),
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
		expect(labels).toContain("Notifications");
		expect(labels).toContain("Agents");
		expect(labels).not.toContain("Groups");
	});

	it("shows the new Terminal, Developer Tools and Keyboard Shortcuts nav items, and the Voice label", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const navItems = container.querySelectorAll(".navItem");
		const labels = Array.from(navItems).map((n) => n.textContent);
		expect(labels).toContain("Terminal");
		expect(labels).toContain("Developer Tools");
		expect(labels).toContain("Keyboard Shortcuts");
		expect(labels).toContain("Voice");
		expect(labels).not.toContain("Dictation");
	});

	it("hides the AI Chat nav item while the feature flag is off", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const navItems = container.querySelectorAll(".navItem");
		const labels = Array.from(navItems).map((n) => n.textContent);
		expect(labels).not.toContain("AI Chat");
		expect(labels).not.toContain("Providers");
	});

	it("shows the shortcut editor when the Keyboard Shortcuts nav item is active", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const navItems = container.querySelectorAll(".navItem");
		const item = Array.from(navItems).find((n) => n.textContent === "Keyboard Shortcuts")!;
		fireEvent.click(item);

		const heading = container.querySelector(".section h3");
		expect(heading!.textContent).toBe("Keyboard Shortcuts");
		// Reused from HelpPanel's KeyboardShortcutsTab — same shortcut rows render here.
		expect(container.querySelectorAll("kbd").length).toBeGreaterThan(0);
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

		// Default is General (Terminal/Power Management moved to the Terminal tab)
		const headings = container.querySelectorAll(".section h3");
		expect(headings.length).toBeGreaterThanOrEqual(3);
		// Use childNodes[0] to get heading text without tooltip content
		const headingTexts = Array.from(headings).map((h) => h.childNodes[0]?.textContent?.trim() ?? "");
		expect(headingTexts).toContain("General");
		expect(headingTexts).toContain("Confirmations");
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
