import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getModifierSymbol } from "../../platform";
import type { AwaitingInputType } from "../../stores/terminals";

const mockCopyPathToClipboard = vi.hoisted(() => vi.fn());
const mockWriteClipboard = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock("../../utils/clipboard", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../utils/clipboard")>()),
	copyPathToClipboard: mockCopyPathToClipboard,
	writeClipboard: mockWriteClipboard,
}));

const { mockOpenLocalPath, mockHandleOpenUrl } = vi.hoisted(() => ({
	mockOpenLocalPath: vi.fn(),
	mockHandleOpenUrl: vi.fn(),
}));
vi.mock("../../utils/openUrl", () => ({
	openLocalPath: mockOpenLocalPath,
	handleOpenUrl: mockHandleOpenUrl,
}));

// Mock Tauri APIs
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/window", () => ({
	getCurrentWindow: vi.fn(() => ({
		listen: vi.fn().mockResolvedValue(vi.fn()),
		setTitle: vi.fn().mockResolvedValue(undefined),
	})),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
	open: vi.fn().mockResolvedValue(null),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({
	openUrl: vi.fn().mockResolvedValue(undefined),
}));

import { TabBar } from "../../components/TabBar/TabBar";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { mdTabsStore } from "../../stores/mdTabs";
import { paneLayoutStore } from "../../stores/paneLayout";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore, type TabOrderingMode } from "../../stores/settings";
import { tabOrderingStore } from "../../stores/tabManager";
import { terminalsStore } from "../../stores/terminals";
import { ptyCaptureStore } from "../../utils/ptyCapture";

describe("TabBar", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		localStorage.clear();
		// Clean up any terminals from previous tests
		for (const id of terminalsStore.getIds()) {
			terminalsStore.remove(id);
		}
		// Clean up repos
		for (const path of repositoriesStore.getPaths()) {
			repositoriesStore.remove(path);
		}
		repositoriesStore.setActive(null);
		// Clean up diff/md tabs
		for (const id of diffTabsStore.getIds()) {
			diffTabsStore.remove(id);
		}
		for (const id of mdTabsStore.getIds()) {
			mdTabsStore.remove(id);
		}
		for (const id of editorTabsStore.getIds()) {
			editorTabsStore.remove(id);
		}
		tabOrderingStore.clear();
		settingsStore.setTabOrderingMode("grouped-by-type");
		// Deactivate global workspace
		if (globalWorkspaceStore.isActive()) {
			globalWorkspaceStore.deactivate();
		}
		for (const id of globalWorkspaceStore.getPromotedIds()) {
			globalWorkspaceStore.unpromote(id);
		}
	});

	afterEach(() => {
		vi.useRealTimers();
		paneLayoutStore._testCancelPendingSave();
		repositoriesStore._testCancelPendingSave();
	});

	function addTerminal(
		overrides: Partial<{
			name: string;
			sessionId: string | null;
			fontSize: number;
			cwd: string | null;
			awaitingInput: AwaitingInputType;
		}> = {},
	) {
		return terminalsStore.add({
			name: overrides.name ?? "Terminal",
			sessionId: overrides.sessionId ?? null,
			fontSize: overrides.fontSize ?? 14,
			cwd: overrides.cwd ?? null,
			awaitingInput: overrides.awaitingInput ?? null,
		});
	}

	it("renders the tmux shim's accent color as a --accent-color style var, and omits it when unset", () => {
		const id = addTerminal();
		terminalsStore.update(id, { accentColor: "blue" });
		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(`[data-tab-id="${id}"]`) as HTMLElement;
		expect(tab.style.getPropertyValue("--accent-color")).toBe("blue");

		const plainId = addTerminal({ name: "Plain" });
		const { container: plainContainer } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const plainTab = plainContainer.querySelector(`[data-tab-id="${plainId}"]`) as HTMLElement;
		expect(plainTab.style.getPropertyValue("--accent-color")).toBe("");
	});

	it("clicking new tab button calls onNewTab directly", () => {
		const onNewTab = vi.fn();
		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={onNewTab}
			/>
		));
		fireEvent.click(container.querySelector(".newBtn")!);
		expect(onNewTab).toHaveBeenCalledTimes(1);
	});

	it("right-clicking new tab button opens split context menu", () => {
		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const btn = container.querySelector(".newBtn")!;
		vi.spyOn(btn, "getBoundingClientRect").mockReturnValue({
			left: 100,
			bottom: 50,
			top: 20,
			right: 150,
			width: 50,
			height: 30,
			x: 100,
			y: 20,
			toJSON: () => {},
		} as DOMRect);
		fireEvent.contextMenu(btn);
		const menus = container.querySelectorAll(".menu");
		expect(menus.length).toBeGreaterThan(0);
		const labels = Array.from(menus[menus.length - 1].querySelectorAll(".label"));
		const labelTexts = labels.map((l) => l.textContent);
		expect(labelTexts).toContain("New Tab");
		expect(labelTexts).toContain("Split Vertically");
		expect(labelTexts).toContain("Split Horizontally");
	});

	it("new tab button has correct title", () => {
		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		expect(container.querySelector(".newBtn")!.getAttribute("title")).toBe(`New Tab (${getModifierSymbol()}T)`);
	});

	it("renders terminal tabs from the store", () => {
		addTerminal({ name: "Tab A" });
		addTerminal({ name: "Tab B" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		expect(tabs.length).toBe(2);
		expect(tabs[0].querySelector(".tabName")!.textContent).toContain("Tab A");
		expect(tabs[1].querySelector(".tabName")!.textContent).toContain("Tab B");
	});

	it("active tab has 'active' class", () => {
		const id1 = addTerminal({ name: "Tab 1" });
		addTerminal({ name: "Tab 2" });
		terminalsStore.setActive(id1);

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		expect(tabs[0].classList.contains("active")).toBe(true);
		expect(tabs[1].classList.contains("active")).toBe(false);
	});

	it("activity flag does NOT produce a visible dot class (busy covers it)", () => {
		const id1 = addTerminal({ name: "Active" });
		const id2 = addTerminal({ name: "Background" });
		terminalsStore.setActive(id1);
		terminalsStore.update(id2, { activity: true });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		// activity flag no longer drives dot color — busy state does
		expect(tabs[0].classList.contains("hasActivity")).toBe(false);
		expect(tabs[1].classList.contains("hasActivity")).toBe(false);
	});

	it("tab awaiting input has 'awaitingInput awaitingQuestion' class", () => {
		const id = addTerminal({ name: "Waiting", awaitingInput: "question" });
		terminalsStore.setActive(id);

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.classList.contains("awaitingInput")).toBe(true);
		expect(tab.classList.contains("awaitingQuestion")).toBe(true);
	});

	it("tab awaiting error input has correct class", () => {
		const id = addTerminal({ name: "Error", awaitingInput: "error" });
		terminalsStore.setActive(id);

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.classList.contains("awaitingInput")).toBe(true);
		expect(tab.classList.contains("awaitingError")).toBe(true);
	});

	it("tab with no awaitingInput has no awaiting classes", () => {
		const id = addTerminal({ name: "Normal" });
		terminalsStore.setActive(id);

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.classList.contains("awaitingInput")).toBe(false);
	});

	it("non-active tab with shellState 'idle' has 'shellIdle' class", () => {
		const id1 = addTerminal({ name: "Active" });
		const id2 = addTerminal({ name: "Idle" });
		terminalsStore.setActive(id1);
		terminalsStore.update(id2, { shellState: "idle" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		expect(tabs[0].classList.contains("shellIdle")).toBe(false);
		expect(tabs[1].classList.contains("shellIdle")).toBe(true);
	});

	it("active tab with shellState 'idle' has 'shellIdle' class", () => {
		const id1 = addTerminal({ name: "Active" });
		terminalsStore.setActive(id1);
		terminalsStore.update(id1, { shellState: "idle" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.classList.contains("shellIdle")).toBe(true);
	});

	it("non-active tab with shellState 'busy' does NOT have 'shellIdle' class", () => {
		const id1 = addTerminal({ name: "Active" });
		const id2 = addTerminal({ name: "Busy" });
		terminalsStore.setActive(id1);
		terminalsStore.update(id2, { shellState: "busy" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		expect(tabs[1].classList.contains("shellIdle")).toBe(false);
	});

	describe("terminal tab status icon respects indicator overrides", () => {
		afterEach(() => {
			settingsStore.resetAllIndicators();
		});

		it("renders the default 'dot' shape (a <circle>) with no override", () => {
			const id = addTerminal({ name: "Busy" });
			terminalsStore.setActive(id);
			terminalsStore.update(id, { shellState: "busy" });

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const icon = container.querySelector(".tabIcon")!;
			expect(icon.querySelector("circle")).not.toBeNull();
			expect(icon.querySelector("rect")).toBeNull();
		});

		it("renders the overridden icon shape (a <rect> for 'square') for the busy state", () => {
			settingsStore.setIndicatorIcon("terminal.busy", "square");
			const id = addTerminal({ name: "Busy" });
			terminalsStore.setActive(id);
			terminalsStore.update(id, { shellState: "busy" });

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const icon = container.querySelector(".tabIcon")!;
			expect(icon.querySelector("rect")).not.toBeNull();
			expect(icon.querySelector("circle")).toBeNull();
		});

		it("prioritizes the awaiting-question icon override over a busy shellState", () => {
			settingsStore.setIndicatorIcon("terminal.question", "square");
			const id = addTerminal({ name: "Waiting", awaitingInput: "question" });
			terminalsStore.setActive(id);
			terminalsStore.update(id, { shellState: "busy" });

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const icon = container.querySelector(".tabIcon")!;
			expect(icon.querySelector("rect")).not.toBeNull();
		});
	});

	it("tab with progress shows bar at correct width (no label)", () => {
		const id = addTerminal({ name: "Progress" });
		terminalsStore.update(id, { progress: { kind: "normal", value: 50 } });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.querySelector(".progressLabel")).toBeNull();
		const bar = tab.querySelector(".progress");
		expect(bar).not.toBeNull();
		expect((bar as HTMLElement).getAttribute("data-kind")).toBe("normal");
		expect((bar as HTMLElement).style.transform).toBe("scaleX(0.5)");
	});

	it("tab with progress=0 shows bar, no label", () => {
		const id = addTerminal({ name: "Zero" });
		terminalsStore.update(id, { progress: { kind: "normal", value: 0 } });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.querySelector(".progressLabel")).toBeNull();
		const bar = tab.querySelector(".progress");
		expect(bar).not.toBeNull();
		expect((bar as HTMLElement).style.transform).toBe("scaleX(0)");
	});

	it("tab with error progress shows bar with error data-kind and its value", () => {
		const id = addTerminal({ name: "Error" });
		terminalsStore.update(id, { progress: { kind: "error", value: 30 } });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const bar = container.querySelector(".tab .progress") as HTMLElement;
		expect(bar).not.toBeNull();
		expect(bar.getAttribute("data-kind")).toBe("error");
		expect(bar.style.transform).toBe("scaleX(0.3)");
	});

	it("tab with error progress and no value shows a full-width bar", () => {
		const id = addTerminal({ name: "ErrorNoValue" });
		terminalsStore.update(id, { progress: { kind: "error", value: null } });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const bar = container.querySelector(".tab .progress") as HTMLElement;
		expect(bar).not.toBeNull();
		expect(bar.getAttribute("data-kind")).toBe("error");
		expect(bar.style.transform).toBe("scaleX(1)");
	});

	it("tab with warning progress shows bar with warning data-kind and its value", () => {
		const id = addTerminal({ name: "Warning" });
		terminalsStore.update(id, { progress: { kind: "warning", value: 65 } });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const bar = container.querySelector(".tab .progress") as HTMLElement;
		expect(bar).not.toBeNull();
		expect(bar.getAttribute("data-kind")).toBe("warning");
		expect(bar.style.transform).toBe("scaleX(0.65)");
	});

	it("tab with indeterminate progress shows bar with no transform (ignores value)", () => {
		const id = addTerminal({ name: "Indeterminate" });
		terminalsStore.update(id, { progress: { kind: "indeterminate", value: 90 } });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const bar = container.querySelector(".tab .progress") as HTMLElement;
		expect(bar).not.toBeNull();
		expect(bar.getAttribute("data-kind")).toBe("indeterminate");
		expect(bar.style.transform).toBe("");
	});

	it("tab with progress=null does not show progress elements", () => {
		addTerminal({ name: "No Progress" });
		// progress defaults to null

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.querySelector(".progressLabel")).toBeNull();
		expect(tab.querySelector(".progress")).toBeNull();
	});

	it("clicking tab calls onTabSelect with correct id", () => {
		const handleSelect = vi.fn();
		addTerminal({ name: "Tab 1" });
		const id2 = addTerminal({ name: "Tab 2" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={handleSelect}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		fireEvent.click(tabs[1]);
		expect(handleSelect).toHaveBeenCalledWith(id2);
	});

	it("clicking close button calls onTabClose and stops propagation", () => {
		const handleSelect = vi.fn();
		const handleClose = vi.fn();
		const id1 = addTerminal({ name: "Tab 1" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={handleSelect}
				onTabClose={handleClose}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const closeBtn = container.querySelector(".tabClose")!;
		fireEvent.click(closeBtn);
		expect(handleClose).toHaveBeenCalledWith(id1);
		// stopPropagation means onTabSelect should NOT be called
		expect(handleSelect).not.toHaveBeenCalled();
	});

	it("when activeRepoPath and activeBranch set, uses branch terminals", () => {
		// Set up a repo with a branch that has specific terminals
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);

		const t1 = addTerminal({ name: "Branch Term 1" });
		const t2 = addTerminal({ name: "Branch Term 2" });
		addTerminal({ name: "Other Term" }); // Not in the branch

		repositoriesStore.setWorkspace(repoPath, "main", {
			branchName: "main",
			terminals: [t1, t2],
		});
		repositoriesStore.setActiveWorkspace(repoPath, "main");

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		// Should only show t1 and t2, not t3
		expect(tabs.length).toBe(2);
		expect(tabs[0].querySelector(".tabName")!.textContent).toContain("Branch Term 1");
		expect(tabs[1].querySelector(".tabName")!.textContent).toContain("Branch Term 2");
	});

	it("with no activeRepoPath, shows all terminals", () => {
		addTerminal({ name: "T1" });
		addTerminal({ name: "T2" });
		addTerminal({ name: "T3" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		expect(container.querySelectorAll(".tab").length).toBe(3);
	});

	it("when global workspace is active, shows only promoted terminals", () => {
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);

		const promoted1 = addTerminal({ name: "Promoted 1" });
		const promoted2 = addTerminal({ name: "Promoted 2" });
		const repoBound = addTerminal({ name: "Repo Term" });

		repositoriesStore.setWorkspace(repoPath, "main", {
			branchName: "main",
			terminals: [repoBound],
		});
		repositoriesStore.setActiveWorkspace(repoPath, "main");

		globalWorkspaceStore.promote(promoted1);
		globalWorkspaceStore.promote(promoted2);
		globalWorkspaceStore.activate();

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		expect(tabs.length).toBe(2);
		const names = Array.from(tabs).map((t) => t.querySelector(".tabName")!.textContent);
		expect(names).toContain("Promoted 1");
		expect(names).toContain("Promoted 2");
		expect(names).not.toContain("Repo Term");
	});

	it("with activeRepoPath but no activeBranch, falls back to all terminals", () => {
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);
		// No activeBranch set

		addTerminal({ name: "T1" });
		addTerminal({ name: "T2" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		expect(container.querySelectorAll(".tab").length).toBe(2);
	});

	it("tab has correct title with 1-based index", () => {
		addTerminal({ name: "First" });
		addTerminal({ name: "Second" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		expect(tabs[0].getAttribute("title")).toBe(`Terminal 1 (${getModifierSymbol()}1)`);
		expect(tabs[1].getAttribute("title")).toBe(`Terminal 2 (${getModifierSymbol()}2)`);
	});

	it("pointerDown + move initiates drag (dragging class appears after threshold)", () => {
		addTerminal({ name: "Tab 1" });
		addTerminal({ name: "Tab 2" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");

		// pointerDown alone should NOT set dragging
		fireEvent.pointerDown(tabs[0], { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
		expect(tabs[0].classList.contains("dragging")).toBe(false);

		// Move past threshold to start drag
		fireEvent.pointerMove(document, { pointerId: 1, clientX: 20, clientY: 10 });
		expect(tabs[0].classList.contains("dragging")).toBe(true);

		// Cleanup
		fireEvent.pointerUp(document, { pointerId: 1, clientX: 20, clientY: 10 });
	});

	it("pointerUp without movement does not trigger drag (click works normally)", () => {
		const handleReorder = vi.fn();
		addTerminal({ name: "Tab 1" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
				onReorder={handleReorder}
			/>
		));
		const tabs = container.querySelectorAll(".tab");

		fireEvent.pointerDown(tabs[0], { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
		fireEvent.pointerUp(document, { pointerId: 1, clientX: 10, clientY: 10 });

		expect(handleReorder).not.toHaveBeenCalled();
		expect(tabs[0].classList.contains("dragging")).toBe(false);
	});

	it("escape cancels drag and resets state", () => {
		addTerminal({ name: "Tab 1" });
		addTerminal({ name: "Tab 2" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");

		fireEvent.pointerDown(tabs[0], { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
		fireEvent.pointerMove(document, { pointerId: 1, clientX: 20, clientY: 10 });
		expect(tabs[0].classList.contains("dragging")).toBe(true);

		fireEvent.keyDown(document, { key: "Escape" });
		expect(tabs[0].classList.contains("dragging")).toBe(false);
	});

	describe("diff tabs", () => {
		/** Set up an active repo+branch so diff/md tab visibility filtering works */
		function setupActiveRepo() {
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "main", { isMain: true, worktreePath: null });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
		}

		it("renders diff tabs", () => {
			setupActiveRepo();
			diffTabsStore.add("/repo", "/repo/file.ts", "M");

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const diffTabs = container.querySelectorAll(".diffTab");
			expect(diffTabs.length).toBe(1);
			expect(diffTabs[0].querySelector(".tabName")!.textContent).toBe("file.ts");
		});

		it("clicking diff tab selects it", () => {
			setupActiveRepo();
			const id = diffTabsStore.add("/repo", "/repo/file.ts", "M");
			const handleSelect = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={handleSelect}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			fireEvent.click(container.querySelector(".diffTab")!);
			expect(handleSelect).toHaveBeenCalledWith(id);
			expect(diffTabsStore.state.activeId).toBe(id);
		});

		it("closing diff tab via close button", () => {
			setupActiveRepo();
			const id = diffTabsStore.add("/repo", "/repo/file.ts", "M");
			const handleClose = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={handleClose}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const closeBtn = container.querySelector(".diffTab .tabClose")!;
			fireEvent.click(closeBtn);
			expect(handleClose).toHaveBeenCalledWith(id);
		});

		it("middle-click closes diff tab", () => {
			setupActiveRepo();
			const id = diffTabsStore.add("/repo", "/repo/file.ts", "M");
			const handleClose = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={handleClose}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			fireEvent(container.querySelector(".diffTab")!, new MouseEvent("auxclick", { button: 1, bubbles: true }));
			expect(handleClose).toHaveBeenCalledWith(id, true);
		});
	});

	describe("markdown tabs", () => {
		function setupActiveRepo() {
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
		}

		it("renders markdown tabs", () => {
			setupActiveRepo();
			mdTabsStore.add("/repo", "/repo/readme.md");

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const mdTabs = container.querySelectorAll(".mdTab");
			expect(mdTabs.length).toBe(1);
			expect(mdTabs[0].querySelector(".tabName")!.textContent).toBe("readme.md");
		});

		it("clicking md tab selects it", () => {
			setupActiveRepo();
			const id = mdTabsStore.add("/repo", "/repo/readme.md");
			const handleSelect = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={handleSelect}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			fireEvent.click(container.querySelector(".mdTab")!);
			expect(handleSelect).toHaveBeenCalledWith(id);
			expect(mdTabsStore.state.activeId).toBe(id);
		});

		it("closing md tab via close button", () => {
			setupActiveRepo();
			const id = mdTabsStore.add("/repo", "/repo/readme.md");
			const handleClose = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={handleClose}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const closeBtn = container.querySelector(".mdTab .tabClose")!;
			fireEvent.click(closeBtn);
			expect(handleClose).toHaveBeenCalledWith(id);
		});
	});

	describe("cross-kind drag reorder", () => {
		/** Set up a repo with one terminal plus one diff and one markdown tab. */
		function setupMixedTabs() {
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "main", { isMain: true, worktreePath: null });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
			const terminalId = addTerminal({ name: "Terminal" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", terminalId);
			const diffId = diffTabsStore.add("/repo", "/repo/change.ts", "M");
			const markdownId = mdTabsStore.add("/repo", "/repo/readme.md");
			return { terminalId, diffId, markdownId };
		}

		/** Drag `sourceEl` onto `targetEl` and release, dropping on the given half. */
		function dragOnto(sourceEl: Element, targetEl: Element, side: "left" | "right") {
			vi.spyOn(targetEl, "getBoundingClientRect").mockReturnValue({
				left: 100,
				right: 200,
				top: 0,
				bottom: 30,
				width: 100,
				height: 30,
				x: 100,
				y: 0,
				toJSON: () => ({}),
			} as DOMRect);
			vi.spyOn(document, "elementFromPoint").mockReturnValue(targetEl);
			const dropX = side === "left" ? 110 : 190;

			fireEvent.pointerDown(sourceEl, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
			fireEvent.pointerMove(document, { pointerId: 1, clientX: dropX, clientY: 10 });
			fireEvent.pointerUp(document, { pointerId: 1, clientX: dropX, clientY: 10 });
		}

		function renderedTabIds(container: HTMLElement): string[] {
			return [...container.querySelectorAll("[data-tab-id]")].map((el) => (el as HTMLElement).dataset.tabId!);
		}

		it("free mode: dropping a diff on a terminal moves it before the terminal", () => {
			const { terminalId, diffId, markdownId } = setupMixedTabs();
			settingsStore.setTabOrderingMode("free");

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			expect(renderedTabIds(container)).toEqual([terminalId, diffId, markdownId]);

			const source = container.querySelector(`[data-tab-id="${diffId}"]`)!;
			const target = container.querySelector(`[data-tab-id="${terminalId}"]`)!;
			dragOnto(source, target, "left");

			expect(renderedTabIds(container)).toEqual([diffId, terminalId, markdownId]);
		});

		it("free mode: dropping a terminal on a markdown tab moves it after the markdown tab", () => {
			const { terminalId, diffId, markdownId } = setupMixedTabs();
			settingsStore.setTabOrderingMode("free");

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));

			const source = container.querySelector(`[data-tab-id="${terminalId}"]`)!;
			const target = container.querySelector(`[data-tab-id="${markdownId}"]`)!;
			dragOnto(source, target, "right");

			expect(renderedTabIds(container)).toEqual([diffId, markdownId, terminalId]);
		});

		it("terminals-first mode: dropping a markdown tab on a diff reorders the non-terminal tabs", () => {
			const { terminalId, diffId, markdownId } = setupMixedTabs();
			settingsStore.setTabOrderingMode("terminals-first");

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			expect(renderedTabIds(container)).toEqual([terminalId, diffId, markdownId]);

			const source = container.querySelector(`[data-tab-id="${markdownId}"]`)!;
			const target = container.querySelector(`[data-tab-id="${diffId}"]`)!;
			dragOnto(source, target, "left");

			expect(renderedTabIds(container)).toEqual([terminalId, markdownId, diffId]);
		});

		it("grouped mode: a same-kind drop still reorders within the kind", () => {
			const { terminalId } = setupMixedTabs();
			const secondDiffId = diffTabsStore.add("/repo", "/repo/other.ts", "M");
			settingsStore.setTabOrderingMode("grouped-by-type");

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const ids = renderedTabIds(container);
			const firstDiffId = ids[1];

			const source = container.querySelector(`[data-tab-id="${secondDiffId}"]`)!;
			const target = container.querySelector(`[data-tab-id="${firstDiffId}"]`)!;
			dragOnto(source, target, "left");

			expect(renderedTabIds(container).slice(0, 3)).toEqual([terminalId, secondDiffId, firstDiffId]);
		});

		it("terminals-first mode: dropping a terminal on a terminal still calls onReorder", () => {
			setupMixedTabs();
			const secondTerminalId = addTerminal({ name: "Terminal 2" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", secondTerminalId);
			settingsStore.setTabOrderingMode("terminals-first");
			const onReorder = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
					onReorder={onReorder}
				/>
			));
			const ids = renderedTabIds(container);

			const source = container.querySelector(`[data-tab-id="${secondTerminalId}"]`)!;
			const target = container.querySelector(`[data-tab-id="${ids[0]}"]`)!;
			dragOnto(source, target, "left");

			expect(onReorder).toHaveBeenCalledWith(1, 0);
		});

		it("closing a tab drops it from the cross-kind order", () => {
			const { terminalId, diffId, markdownId } = setupMixedTabs();
			settingsStore.setTabOrderingMode("free");

			mdTabsStore.remove(markdownId);

			expect(tabOrderingStore.getOrdered(new Set([terminalId, diffId]))).toEqual([terminalId, diffId]);
			expect(tabOrderingStore.state.order).not.toContain(markdownId);
		});
	});

	describe("tab-kind parity across ordering modes", () => {
		it.each(["grouped-by-type", "free"] satisfies TabOrderingMode[])(
			"renders and selects every tab kind in %s mode",
			(mode) => {
				repositoriesStore.add({ path: "/repo", displayName: "repo" });
				repositoriesStore.setWorkspace("/repo", "main", { isMain: true, worktreePath: null });
				repositoriesStore.setActive("/repo");
				repositoriesStore.setActiveWorkspace("/repo", "main");

				const terminalId = addTerminal({ name: "Terminal parity" });
				repositoriesStore.addTerminalToWorkspace("/repo", "main", terminalId);
				const diffId = diffTabsStore.add("/repo", "/repo/change.ts", "M");
				const markdownId = mdTabsStore.add("/repo", "/repo/readme.md");
				const editorId = editorTabsStore.add("/repo", "/repo/edit.ts");
				settingsStore.setTabOrderingMode(mode);

				const onTabSelect = vi.fn();
				const { container } = render(() => (
					<TabBar
						onTabSelect={onTabSelect}
						onTabClose={() => {}}
						onCloseOthers={() => {}}
						onCloseToRight={() => {}}
						onNewTab={() => {}}
					/>
				));

				for (const [id, label] of [
					[terminalId, "Terminal parity"],
					[diffId, "change.ts"],
					[markdownId, "readme.md"],
					[editorId, "edit.ts"],
				] as const) {
					const tab = container.querySelector(`[data-tab-id="${id}"]`);
					expect(tab, `${mode}:${id}`).not.toBeNull();
					expect(tab!.querySelector(".tabName")?.textContent).toContain(label);
					fireEvent.click(tab!);
					expect(onTabSelect).toHaveBeenLastCalledWith(id);
				}
			},
		);
	});

	describe("tab rename", () => {
		it("double-click enters edit mode", () => {
			const id = addTerminal({ name: "My Tab" });
			terminalsStore.setActive(id);

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.dblClick(tab);
			const input = tab.querySelector(".tabNameInput");
			expect(input).not.toBeNull();
		});

		it("Enter key commits rename", () => {
			const id = addTerminal({ name: "Old Name" });
			terminalsStore.setActive(id);

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.dblClick(tab);
			const input = tab.querySelector(".tabNameInput") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "New Name" } });
			fireEvent.keyDown(input, { key: "Enter" });
			expect(terminalsStore.get(id)?.name).toBe("New Name");
		});

		it("Escape key cancels rename", () => {
			const id = addTerminal({ name: "Original" });
			terminalsStore.setActive(id);

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.dblClick(tab);
			const input = tab.querySelector(".tabNameInput") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "Changed" } });
			fireEvent.keyDown(input, { key: "Escape" });
			// Name should remain original (Escape cancels)
			expect(terminalsStore.get(id)?.name).toBe("Original");
		});
	});

	// The tab stores keep `filePath` RELATIVE to the tab's filesystem root. Copy Path
	// used to hand the bare `filePath` to the clipboard, so a tab yielded `src/a.ts`
	// while the File Browser yielded the full path for the same file — useless
	// anywhere the consumer's cwd is not the repo root.
	describe("Copy Path copies the absolute path", () => {
		function setupRepo() {
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "main", { isMain: true, worktreePath: null });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
		}

		function clickCopyPath(container: HTMLElement, tabSelector: string) {
			fireEvent.contextMenu(container.querySelector(tabSelector)!);
			const item = Array.from(container.querySelectorAll(".menu .item")).find(
				(i) => i.querySelector(".label")?.textContent === "Copy Path",
			)!;
			fireEvent.click(item);
		}

		const renderTabBar = () =>
			render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));

		beforeEach(() => {
			mockCopyPathToClipboard.mockClear();
			mockOpenLocalPath.mockClear();
			mockHandleOpenUrl.mockClear();
			setupRepo();
		});

		it("joins a diff tab's relative path onto its repo root", () => {
			diffTabsStore.add("/repo", "src/file.ts", "M");
			const { container } = renderTabBar();
			clickCopyPath(container, ".diffTab");
			expect(mockCopyPathToClipboard).toHaveBeenCalledWith("/repo/src/file.ts");
		});

		it("joins a markdown tab's relative path onto its worktree root", () => {
			mdTabsStore.add("/repo", "docs/readme.md", "/wt/feature");
			const { container } = renderTabBar();
			clickCopyPath(container, ".mdTab");
			expect(mockCopyPathToClipboard).toHaveBeenCalledWith("/wt/feature/docs/readme.md");
		});

		it("joins an editor tab's relative path onto its fs root", () => {
			editorTabsStore.add("/repo", "src/main.rs", undefined, { fsRoot: "/wt/feature" });
			const { container } = renderTabBar();
			clickCopyPath(container, ".editTab");
			expect(mockCopyPathToClipboard).toHaveBeenCalledWith("/wt/feature/src/main.rs");
		});

		// `ui action=tab url=file://…` renders through srcdoc because the iframe
		// sandbox blocks file://, so the panel looks memory-only. It is not — the
		// url is the only record of where the document came from.
		it("recovers the path of a file:// panel tab", () => {
			mdTabsStore.openUiTab("bench", "Dev benchmark 90d", "", false, "file:///tmp/report%20one.html", false);
			const { container } = renderTabBar();
			clickCopyPath(container, ".panelTab");
			expect(mockCopyPathToClipboard).toHaveBeenCalledWith("/tmp/report one.html");
		});

		// `handleOpenUrl` allows http/https/mailto only — terminal output is
		// untrusted — so a file:// panel must go to the OS opener instead, or the
		// menu item does nothing but log "Blocked URL with disallowed scheme".
		it("sends a file:// panel to the OS opener, not the URL allowlist", () => {
			mdTabsStore.openUiTab("bench2", "Report", "", false, "file:///tmp/report.html", false);
			const { container } = renderTabBar();
			fireEvent.contextMenu(container.querySelector(".panelTab")!);
			const item = Array.from(container.querySelectorAll(".menu .item")).find(
				(i) => i.querySelector(".label")?.textContent === "Open in Browser",
			)!;
			fireEvent.click(item);
			expect(mockOpenLocalPath).toHaveBeenCalledWith("/tmp/report.html");
			expect(mockHandleOpenUrl).not.toHaveBeenCalled();
		});

		it("keeps an http panel on the URL allowlist", () => {
			mdTabsStore.openUiTab("web", "Dashboard", "", false, "http://127.0.0.1:14319", false);
			const { container } = renderTabBar();
			fireEvent.contextMenu(container.querySelector(".panelTab")!);
			const item = Array.from(container.querySelectorAll(".menu .item")).find(
				(i) => i.querySelector(".label")?.textContent === "Open in Browser",
			)!;
			fireEvent.click(item);
			expect(mockHandleOpenUrl).toHaveBeenCalledWith("http://127.0.0.1:14319");
			expect(mockOpenLocalPath).not.toHaveBeenCalled();
		});

		it("offers no Copy Path for a panel tab with inline HTML only", () => {
			mdTabsStore.openUiTab("inline", "Inline panel", "<p>hi</p>", false, undefined, false);
			const { container } = renderTabBar();
			fireEvent.contextMenu(container.querySelector(".panelTab")!);
			const labels = Array.from(container.querySelectorAll(".menu .item .label")).map((l) => l.textContent);
			expect(labels).not.toContain("Copy Path");
		});

		it("leaves an already-absolute path from an external file alone", () => {
			editorTabsStore.add("/repo", "/etc/hosts");
			const { container } = renderTabBar();
			clickCopyPath(container, ".editTab");
			expect(mockCopyPathToClipboard).toHaveBeenCalledWith("/etc/hosts");
		});
	});

	describe("alias context menu item", () => {
		const renderTabBar = () =>
			render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));

		function aliasItem(container: HTMLElement): Element | undefined {
			fireEvent.contextMenu(container.querySelector(".tab")!);
			return Array.from(container.querySelectorAll(".menu .item")).find((item) =>
				item.querySelector(".label")?.textContent?.startsWith("Alias:"),
			);
		}

		beforeEach(() => {
			mockWriteClipboard.mockClear();
		});

		// The alias is the address another agent has to be told, so it has to be
		// readable and copyable from the tab that owns it — otherwise it is only
		// ever visible inside an MCP payload.
		it("shows the alias and copies it to the clipboard", () => {
			const id = addTerminal({ name: "Tab 1" });
			terminalsStore.update(id, { alias: "tu-2" });

			const { container } = renderTabBar();
			const item = aliasItem(container);

			expect(item?.querySelector(".label")?.textContent).toBe("Alias: tu-2");
			fireEvent.click(item!);
			expect(mockWriteClipboard).toHaveBeenCalledWith("tu-2");
		});

		it("is absent while the session has no alias yet", () => {
			addTerminal({ name: "Tab 1" });
			const { container } = renderTabBar();
			expect(aliasItem(container)).toBeUndefined();
		});
	});

	describe("context menu", () => {
		it("right-click opens context menu", () => {
			addTerminal({ name: "Tab 1" });

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.contextMenu(tab);
			const menu = container.querySelector(".menu");
			expect(menu).not.toBeNull();
		});
	});

	describe("new tab menu", () => {
		it("disables split when no active terminal", () => {
			// Clear all terminals so there's no activeId
			for (const id of terminalsStore.getIds()) {
				terminalsStore.remove(id);
			}

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const btn = container.querySelector(".newBtn")!;
			vi.spyOn(btn, "getBoundingClientRect").mockReturnValue({
				left: 100,
				bottom: 50,
				top: 20,
				right: 150,
				width: 50,
				height: 30,
				x: 100,
				y: 20,
				toJSON: () => {},
			} as DOMRect);
			fireEvent.contextMenu(btn);
			const menus = container.querySelectorAll(".menu");
			const menu = menus[menus.length - 1];
			const items = Array.from(menu.querySelectorAll(".item"));
			const splitV = items.find((i) => i.textContent?.includes("Split Vertically"));
			const splitH = items.find((i) => i.textContent?.includes("Split Horizontally"));
			expect(splitV).toBeDefined();
			expect(splitH).toBeDefined();
			expect(splitV!.classList.contains("disabled")).toBe(true);
			expect(splitH!.classList.contains("disabled")).toBe(true);
		});

		it("calls onSplitVertical when Split Vertically is clicked", () => {
			const handleSplit = vi.fn();
			const id = addTerminal({ name: "T1" });
			terminalsStore.setActive(id);

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
					onSplitVertical={handleSplit}
				/>
			));
			const btn = container.querySelector(".newBtn")!;
			vi.spyOn(btn, "getBoundingClientRect").mockReturnValue({
				left: 100,
				bottom: 50,
				top: 20,
				right: 150,
				width: 50,
				height: 30,
				x: 100,
				y: 20,
				toJSON: () => {},
			} as DOMRect);
			fireEvent.contextMenu(btn);
			const menus = container.querySelectorAll(".menu");
			const menu = menus[menus.length - 1];
			const splitBtn = Array.from(menu.querySelectorAll(".item")).find((i) =>
				i.textContent?.includes("Split Vertically"),
			);
			fireEvent.click(splitBtn!);
			expect(handleSplit).toHaveBeenCalledOnce();
		});
	});

	describe("tab close", () => {
		it("close button removes the terminal", () => {
			const id1 = addTerminal({ name: "T1" });
			addTerminal({ name: "T2" });
			terminalsStore.setActive(id1);

			const handleClose = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={handleClose}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));

			const closeBtn = container.querySelector(".tabClose")!;
			fireEvent.click(closeBtn);

			expect(handleClose).toHaveBeenCalledWith(id1);
		});
	});

	describe("quick switcher badges", () => {
		it("shows shortcut badges when quickSwitcherActive", () => {
			addTerminal({ name: "Tab 1" });
			addTerminal({ name: "Tab 2" });

			const { container } = render(() => (
				<TabBar
					quickSwitcherActive={true}
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const badges = container.querySelectorAll(".shortcutBadge");
			expect(badges.length).toBe(2);
		});
	});

	describe("middle-click to close", () => {
		it("middle-click on terminal tab calls onTabClose", () => {
			const handleClose = vi.fn();
			const handleSelect = vi.fn();
			const id1 = addTerminal({ name: "Tab 1" });

			const { container } = render(() => (
				<TabBar
					onTabSelect={handleSelect}
					onTabClose={handleClose}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent(tab, new MouseEvent("auxclick", { button: 1, bubbles: true }));
			expect(handleClose).toHaveBeenCalledWith(id1, true);
			expect(handleSelect).not.toHaveBeenCalled();
		});

		it("right-click auxclick does not close tab", () => {
			const handleClose = vi.fn();
			addTerminal({ name: "Tab 1" });

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={handleClose}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent(tab, new MouseEvent("auxclick", { button: 2, bubbles: true }));
			expect(handleClose).not.toHaveBeenCalled();
		});
	});

	describe("move to worktree context menu", () => {
		function setupRepoWithWorktrees() {
			// Create a repo with main branch and a worktree branch
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "main", { isMain: true, worktreePath: null });
			repositoriesStore.setWorkspace("/repo", "feature-a", { isMain: false, worktreePath: "/repo-wt/feature-a" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
		}

		it("shows Move to Worktree submenu when repo has multiple worktrees", () => {
			setupRepoWithWorktrees();
			const termId = addTerminal({ name: "T1", sessionId: "sess-1" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", termId);
			terminalsStore.setActive(termId);

			const getTargets = () => [{ branchName: "feature-a", path: "/repo-wt/feature-a" }];
			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
					getWorktreeTargets={getTargets}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.contextMenu(tab);

			const menuItems = container.querySelectorAll(".menu .item");
			const moveItem = Array.from(menuItems).find((i) => i.textContent?.includes("Move to Worktree"));
			expect(moveItem).not.toBeNull();
		});

		it("hides Move to Worktree when no worktree targets available", () => {
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "main", { isMain: true, worktreePath: null });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
			const termId = addTerminal({ name: "T1", sessionId: "sess-1" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", termId);
			terminalsStore.setActive(termId);

			const getTargets = () => [] as Array<{ branchName: string; path: string }>;
			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
					getWorktreeTargets={getTargets}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.contextMenu(tab);

			const menuItems = container.querySelectorAll(".menu .item");
			const moveItem = Array.from(menuItems).find((i) => i.textContent?.includes("Move to Worktree"));
			expect(moveItem).toBeUndefined();
		});

		it("calls onMoveToWorktree with correct args when worktree is selected", () => {
			setupRepoWithWorktrees();
			const handleMove = vi.fn();
			const termId = addTerminal({ name: "T1", sessionId: "sess-1" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", termId);
			terminalsStore.setActive(termId);

			const getTargets = () => [{ branchName: "feature-a", path: "/repo-wt/feature-a" }];
			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
					getWorktreeTargets={getTargets}
					onMoveToWorktree={handleMove}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.contextMenu(tab);

			// Find the "Move to Worktree" parent item and hover to open submenu
			const menuItems = container.querySelectorAll(".menu .itemWrap");
			const moveWrap = Array.from(menuItems).find((i) => i.textContent?.includes("Move to Worktree"));
			expect(moveWrap).not.toBeNull();
			fireEvent.mouseEnter(moveWrap!);

			// Find the submenu item "feature-a"
			const submenuItems = moveWrap!.querySelectorAll(".submenu .item");
			const featureItem = Array.from(submenuItems).find((i) => i.textContent?.includes("feature-a"));
			expect(featureItem).not.toBeNull();
			fireEvent.click(featureItem!);

			expect(handleMove).toHaveBeenCalledWith(termId, "/repo-wt/feature-a");
		});
	});

	describe("diagnostics capture badge", () => {
		afterEach(() => {
			// ptyCaptureStore's status signal is module-level state, not reset by
			// the outer beforeEach (which only clears terminal/repo/tab stores) —
			// clear it here so a capture "on" from one test can't leak into the next.
			ptyCaptureStore.applyStatus({ enabled: false });
		});

		it("shows the badge on the exact filtered session, not on others", () => {
			const capturedId = addTerminal({ name: "Captured", sessionId: "sess-captured" });
			const otherId = addTerminal({ name: "Other", sessionId: "sess-other" });
			ptyCaptureStore.applyStatus({ enabled: true, session_filter: "sess-captured" });

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));

			const capturedTab = container.querySelector(`[data-tab-id="${capturedId}"]`) as HTMLElement;
			const otherTab = container.querySelector(`[data-tab-id="${otherId}"]`) as HTMLElement;
			expect(capturedTab.querySelector('[title="Diagnostics capture recording"]')).not.toBeNull();
			expect(otherTab.querySelector('[title="Diagnostics capture recording"]')).toBeNull();
		});

		it("shows the badge on every tab when the tap has no session filter", () => {
			const idA = addTerminal({ name: "A", sessionId: "sess-a" });
			const idB = addTerminal({ name: "B", sessionId: "sess-b" });
			ptyCaptureStore.applyStatus({ enabled: true, session_filter: null });

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));

			const tabA = container.querySelector(`[data-tab-id="${idA}"]`) as HTMLElement;
			const tabB = container.querySelector(`[data-tab-id="${idB}"]`) as HTMLElement;
			expect(tabA.querySelector('[title="Diagnostics capture recording"]')).not.toBeNull();
			expect(tabB.querySelector('[title="Diagnostics capture recording"]')).not.toBeNull();
		});

		it("shows no badge when the tap is disabled", () => {
			const id = addTerminal({ name: "Idle", sessionId: "sess-idle" });
			ptyCaptureStore.applyStatus({ enabled: false });

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));

			const tab = container.querySelector(`[data-tab-id="${id}"]`) as HTMLElement;
			expect(tab.querySelector('[title="Diagnostics capture recording"]')).toBeNull();
		});
	});
});
