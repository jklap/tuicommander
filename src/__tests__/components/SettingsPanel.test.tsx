import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";

vi.mock("../../stores/settings", () => ({
	settingsStore: {
		state: {
			ide: "vscode",
			font: "JetBrains Mono",
			defaultFontSize: 12,
			confirmBeforeQuit: true,
			confirmBeforeClosingTab: true,
			doubleClickAction: "smart",
			wordSelectionMode: "characters",
			wordSeparators: " \"'`(){}[]<>|;:,.!?@#$%^&*~=+/\\",
			wordSelectionRegex: "",
			smartSelectionRules: [],
		},
		setShell: vi.fn(),
		setIde: vi.fn(),
		setFont: vi.fn(),
		setConfirmBeforeQuit: vi.fn(),
		setConfirmBeforeClosingTab: vi.fn(),
		setDoubleClickAction: vi.fn(),
		setWordSelectionMode: vi.fn(),
		setWordSeparators: vi.fn(),
		setWordSelectionRegex: vi.fn(),
		setSmartSelectionRules: vi.fn(),
		isAiChatEnabled: vi.fn().mockReturnValue(false),
		isAcpConfigured: vi.fn().mockReturnValue(false),
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
				sound_choices: {
					question: { preset: "default", custom_path: null },
					error: { preset: "default", custom_path: null },
					completion: { preset: "default", custom_path: null },
					warning: { preset: "default", custom_path: null },
					info: { preset: "default", custom_path: null },
					attention: { preset: "default", custom_path: null },
				},
			},
		},
		setEnabled: vi.fn(),
		setVolume: vi.fn(),
		setSoundEnabled: vi.fn(),
		setSoundChoice: vi.fn(),
		testSound: vi.fn(),
		reset: vi.fn(),
	},
}));

vi.mock("../../stores/ui", () => ({
	uiStore: {
		state: {
			settingsNavWidth: 180,
			lastSettingsTab: null,
		},
		setSettingsNavWidth: vi.fn(),
		setLastSettingsTab: vi.fn(),
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
import { settingsStore } from "../../stores/settings";

/** Nav items keyed by the group label row above them, in rendered order. */
function navGroups(container: HTMLElement): Record<string, string[]> {
	const groups: Record<string, string[]> = {};
	let current = "";
	for (const el of container.querySelectorAll(".navLabel, .navItem")) {
		const text = el.textContent ?? "";
		if (el.classList.contains("navLabel")) {
			current = text;
			groups[current] = [];
		} else {
			(groups[current] ??= []).push(text);
		}
	}
	return groups;
}

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

	it("shows the Terminal and Keyboard Shortcuts nav items and the Voice label, and no Developer Tools page", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const navItems = container.querySelectorAll(".navItem");
		const labels = Array.from(navItems).map((n) => n.textContent);
		expect(labels).toContain("Terminal");
		expect(labels).not.toContain("Developer Tools");
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

		// Default is General (terminal fields moved to the Terminal tab)
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
		expect(navGroups(container).REPOSITORIES).toEqual(["Alpha", "Beta"]);
	});

	it("puts every global page under its task group, one direct item each", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		expect(navGroups(container)).toEqual({
			Application: ["General", "Appearance", "Notifications"],
			Workspace: ["Terminal", "Smart Selection", "Keyboard Shortcuts", "Git & GitHub"],
			AI: ["Agents", "Voice", "Smart Prompts"],
			Integrations: ["MCP", "Remote Access", "Remote Machines", "Telegram", "Plugins"],
			REPOSITORIES: ["Alpha", "Beta"],
		});
	});

	it("opens General for the retired developer-tools key, so an old deep link still lands on a page", () => {
		// tuic://settings?tab=developer-tools and any caller written before the
		// page folded into General must not open a blank content area.
		const { container } = render(() => (
			<SettingsPanel visible={true} onClose={() => {}} initialTab="developer-tools" />
		));
		expect(container.querySelector(".navItem.active")!.textContent).toBe("General");
		const headingTexts = Array.from(container.querySelectorAll(".section h3")).map(
			(h) => h.childNodes[0]?.textContent?.trim() ?? "",
		);
		expect(headingTexts).toContain("General");
		expect(headingTexts).toContain("IDE");
	});

	it("opens the successor page for the tab keys this reorganization retired", () => {
		// services split into MCP / Remote Access / Remote Machines, and providers
		// became AI Chat; an old tuic://settings?tab=… link must land on a page.
		vi.mocked(settingsStore.isAiChatEnabled).mockReturnValue(true);
		try {
			for (const [retired, label] of [
				["services", "MCP"],
				["providers", "AI Chat"],
			]) {
				const { container, unmount } = render(() => (
					<SettingsPanel visible={true} onClose={() => {}} initialTab={retired} />
				));
				expect(container.querySelector(".navItem.active")?.textContent, retired).toBe(label);
				unmount();
			}
		} finally {
			vi.mocked(settingsStore.isAiChatEnabled).mockReturnValue(false);
		}
	});

	it("lists AI Chat under AI when the feature flag is on", () => {
		vi.mocked(settingsStore.isAiChatEnabled).mockReturnValue(true);
		try {
			const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
			expect(navGroups(container).AI).toEqual(["Agents", "AI Chat", "Voice", "Smart Prompts"]);
		} finally {
			vi.mocked(settingsStore.isAiChatEnabled).mockReturnValue(false);
		}
	});

	it("opens a repository's settings from its grouped nav entry", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const beta = Array.from(container.querySelectorAll(".navItemRepo")).find((n) => n.textContent === "Beta")!;
		fireEvent.click(beta);
		expect(container.querySelector(".navItem.active")!.textContent).toBe("Beta");
		expect(container.querySelector(".section h3")!.textContent).toBe("Repository");
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

	it("fetches list_base_ref_options for the active repo and threads it into RepoWorktreeTab's Branch From dropdown", async () => {
		mockInvoke.mockImplementation(async (cmd: string) => {
			if (cmd === "list_base_ref_options") {
				return [
					{ name: "main", kind: "local", is_default: true },
					{ name: "trunk", kind: "local", is_default: false },
				];
			}
			return undefined;
		});

		const { container } = render(() => (
			<SettingsPanel visible={true} onClose={() => {}} context={{ kind: "repo", repoPath: "/repo/alpha" }} />
		));

		expect(mockInvoke).toHaveBeenCalledWith("list_base_ref_options", { repoPath: "/repo/alpha" });

		await waitFor(() => {
			const select = Array.from(container.querySelectorAll("select")).find((s) =>
				Array.from(s.options).some((o) => o.value === "trunk"),
			);
			expect(select).toBeTruthy();
		});

		mockInvoke.mockReset().mockResolvedValue(undefined);
	});

	it("does not crash the panel when list_base_ref_options fails — Branch From just has no dynamic options", async () => {
		mockInvoke.mockImplementation(async (cmd: string) => {
			if (cmd === "list_base_ref_options") throw new Error("boom");
			return undefined;
		});

		const { container } = render(() => (
			<SettingsPanel visible={true} onClose={() => {}} context={{ kind: "repo", repoPath: "/repo/alpha" }} />
		));

		await waitFor(() => {
			expect(mockInvoke).toHaveBeenCalledWith("list_base_ref_options", { repoPath: "/repo/alpha" });
		});
		// Panel still renders normally — the failure doesn't propagate as an unhandled error.
		const h3 = container.querySelector(".section h3");
		expect(h3!.textContent).toBe("Repository");

		mockInvoke.mockReset().mockResolvedValue(undefined);
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
