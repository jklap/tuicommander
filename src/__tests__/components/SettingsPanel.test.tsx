import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";

// A real solid-js signal (created inside the factory via a dynamic import, not
// the top-level one above — see mdkb memory "solid signal in a vi.mock factory
// dual-instance gotcha") so the ai-chat-auto-reset effect's `createEffect`
// actually tracks it and re-runs when a test flips it, the same as it would
// against the real store's reactive state.
const aiChatEnabledBox = vi.hoisted(() => ({
	get: (): boolean => false,
	set: (_v: boolean) => {},
}));

vi.mock("../../stores/settings", async () => {
	const { createSignal: createSignalInFactory } = await import("solid-js");
	const [enabled, setEnabled] = createSignalInFactory(false);
	aiChatEnabledBox.get = enabled;
	aiChatEnabledBox.set = setEnabled;

	return {
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
				osc1337FocusAttention: true,
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
			setOsc1337FocusAttention: vi.fn(),
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
			isAiChatEnabled: () => aiChatEnabledBox.get(),
		},
		IDE_NAMES: { vscode: "VS Code", cursor: "Cursor" },
		FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono", "Fira Code": "Fira Code" },
	};
});

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

vi.mock("../../stores/ui", () => {
	const state: { settingsNavWidth: number; lastSettingsTab: string | null } = {
		settingsNavWidth: 180,
		lastSettingsTab: null,
	};
	return {
		uiStore: {
			state,
			setSettingsNavWidth: vi.fn(),
			setLastSettingsTab: vi.fn((tab: string | null) => {
				state.lastSettingsTab = tab;
			}),
		},
	};
});

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
import { uiStore } from "../../stores/ui";

describe("SettingsPanel", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		uiStore.state.lastSettingsTab = null;
		aiChatEnabledBox.set(false);
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

	it("remembers the last selected pane across a close+reopen", () => {
		const [visible, setVisible] = createSignal(true);
		const { container } = render(() => <SettingsPanel visible={visible()} onClose={() => {}} />);

		const navItems = () => container.querySelectorAll(".navItem");
		const notificationsItem = Array.from(navItems()).find((n) => n.textContent === "Notifications")!;
		fireEvent.click(notificationsItem);
		expect(container.querySelector(".navItem.active")!.textContent).toBe("Notifications");

		// Close and reopen the panel (props.visible false -> true) without an
		// explicit initialTab/context — it should land back on Notifications
		// instead of resetting to General.
		setVisible(false);
		setVisible(true);
		expect(container.querySelector(".navItem.active")!.textContent).toBe("Notifications");
	});

	it("an explicit repo context still wins over a remembered global pane", () => {
		uiStore.state.lastSettingsTab = "notifications";
		const { container } = render(() => (
			<SettingsPanel visible={true} onClose={() => {}} context={{ kind: "repo", repoPath: "/repo/alpha" }} />
		));
		const activeItem = container.querySelector(".navItem.active");
		expect(activeItem!.textContent).toBe("Alpha");
	});

	it("ignores a remembered pane that no longer exists in the nav", () => {
		uiStore.state.lastSettingsTab = "repo:/repo/gone";
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		const activeItem = container.querySelector(".navItem.active");
		expect(activeItem!.textContent).toBe("General");
	});

	describe("settings search", () => {
		it("typing a query replaces the tab content with matching results", () => {
			const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
			const input = container.querySelector(".searchInput") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "copy on select" } });

			expect(container.querySelector(".searchResults")).not.toBeNull();
			expect(container.textContent).toContain("Copy on select");
			// The General tab's own content (still the active nav item) must not
			// render underneath the results.
			expect(container.textContent).not.toContain("Power Management");
		});

		it("shows an empty state when nothing matches", () => {
			const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
			const input = container.querySelector(".searchInput") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "zzzznonexistentzzzz" } });
			expect(container.querySelector(".searchEmpty")).not.toBeNull();
		});

		it("selecting a result switches tab, clears the query, and remembers the pane", () => {
			const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
			const input = container.querySelector(".searchInput") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "copy on select" } });

			const result = container.querySelector(".searchResultItem") as HTMLButtonElement;
			expect(result).toBeTruthy();
			fireEvent.click(result);

			expect((container.querySelector(".searchInput") as HTMLInputElement).value).toBe("");
			expect(container.querySelector(".navItem.active")!.textContent).toBe("Terminal");
			expect(uiStore.setLastSettingsTab).toHaveBeenCalledWith("terminal");
		});

		it("clicking a nav item directly clears an active search query", () => {
			const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
			const input = container.querySelector(".searchInput") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "copy on select" } });
			expect(container.querySelector(".searchResults")).not.toBeNull();

			const navItems = container.querySelectorAll(".navItem");
			const notificationsItem = Array.from(navItems).find((n) => n.textContent === "Notifications")!;
			fireEvent.click(notificationsItem);

			expect((container.querySelector(".searchInput") as HTMLInputElement).value).toBe("");
			expect(container.querySelector(".searchResults")).toBeNull();
		});

		it("the clear button empties the query", () => {
			const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
			const input = container.querySelector(".searchInput") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "copy on select" } });
			const clearBtn = container.querySelector(".searchClear") as HTMLButtonElement;
			expect(clearBtn).toBeTruthy();
			fireEvent.click(clearBtn);
			expect((container.querySelector(".searchInput") as HTMLInputElement).value).toBe("");
			expect(container.querySelector(".searchResults")).toBeNull();
		});

		it("whitespace-only input is treated the same as an empty query", () => {
			const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
			const input = container.querySelector(".searchInput") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "    " } });
			// Trimmed to empty, so this must NOT enter search mode — the active
			// tab's own content stays visible, consistent with searchSettings()
			// itself also treating a whitespace-only query as empty.
			expect(container.querySelector(".searchResults")).toBeNull();
			expect(container.querySelector(".navItem.active")!.textContent).toBe("General");
		});

		it("hides repo settings content while searching, from a repo nav context", () => {
			const { container } = render(() => (
				<SettingsPanel visible={true} onClose={() => {}} context={{ kind: "repo", repoPath: "/repo/alpha" }} />
			));
			// Sanity: repo content is shown before any search.
			expect(container.querySelector(".section h3")!.textContent).toBe("Repository");

			const input = container.querySelector(".searchInput") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "copy on select" } });
			expect(container.textContent).not.toContain("Repository");
			expect(container.querySelector(".searchResults")).not.toBeNull();

			// Clearing the query restores the repo content underneath.
			fireEvent.input(input, { target: { value: "" } });
			expect(container.querySelector(".section h3")!.textContent).toBe("Repository");
		});
	});

	describe("AI Chat tab auto-reset (#1376-7333)", () => {
		it("falling back to General also updates lastSettingsTab, not just the visible tab", () => {
			aiChatEnabledBox.set(true);
			const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);

			const navItems = () => container.querySelectorAll(".navItem");
			const aiChatItem = Array.from(navItems()).find((n) => n.textContent === "AI Chat")!;
			fireEvent.click(aiChatItem);
			expect(container.querySelector(".navItem.active")!.textContent).toBe("AI Chat");
			expect(uiStore.setLastSettingsTab).toHaveBeenCalledWith("ai-chat");

			// The experimental flag flips off while AI Chat is still the active tab.
			aiChatEnabledBox.set(false);

			expect(container.querySelector(".navItem.active")!.textContent).toBe("General");
			// The bug this guards: without this, lastSettingsTab stays "ai-chat" —
			// invisible today (resolveInitialTab validates against buildNavItems,
			// which drops "ai-chat" while disabled) but resurfaces the moment AI
			// Chat is re-enabled before Settings is reopened.
			expect(uiStore.setLastSettingsTab).toHaveBeenCalledWith("general");
			expect(uiStore.state.lastSettingsTab).toBe("general");
		});
	});
});
