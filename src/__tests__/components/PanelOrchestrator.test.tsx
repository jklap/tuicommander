import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Every child panel is stubbed to capture the `visible` prop it receives. The
// unit under test is PanelOrchestrator's visibility wiring, not the panels
// themselves.
const h = vi.hoisted(() => ({
	fileBrowser: [] as Array<{ visible: unknown }>,
	gitPanel: [] as Array<{ visible: unknown }>,
	markdown: [] as Array<{ visible: unknown }>,
}));

vi.mock("../../components/FileBrowserPanel", () => ({
	FileBrowserPanel: (props: { visible: unknown }) => {
		h.fileBrowser.push({ visible: props.visible });
		return null;
	},
}));
vi.mock("../../components/GitPanel/GitPanel", () => ({
	GitPanel: (props: { visible: unknown }) => {
		h.gitPanel.push(props); // live props: `visible` is a reactive getter
		return null;
	},
}));
vi.mock("../../components/MarkdownPanel", () => ({
	MarkdownPanel: (props: { visible: unknown }) => {
		h.markdown.push({ visible: props.visible });
		return null;
	},
}));
vi.mock("../../components/NotesPanel", () => ({ NotesPanel: () => null }));
vi.mock("../../components/OutlinePanel", () => ({ OutlinePanel: () => null }));
vi.mock("../../components/ReferencesPanel", () => ({ ReferencesPanel: () => null }));
vi.mock("../../components/AIChatPanel", () => ({ AIChatPanel: () => null }));
vi.mock("../../components/AiTriagePanel", () => ({ AiTriagePanel: () => null }));

import { PanelOrchestrator } from "../../components/PanelOrchestrator";
import { globalWorkspaceStore, MANUAL_SCOPE } from "../../stores/globalWorkspace";
import { uiStore } from "../../stores/ui";

function renderOrchestrator() {
	return render(() => <PanelOrchestrator repoPath="/repo" fsRoot={null} onFileOpen={() => {}} />);
}

describe("PanelOrchestrator", () => {
	beforeEach(() => {
		h.fileBrowser = [];
		h.gitPanel = [];
		h.markdown = [];
		uiStore.setFileBrowserPanelVisible(false);
		uiStore.setGitPanelVisible(false);
		uiStore.setMarkdownPanelVisible(false);
		if (globalWorkspaceStore.isActive()) globalWorkspaceStore.deactivate();
		globalWorkspaceStore.setScope(MANUAL_SCOPE);
	});

	afterEach(() => {
		uiStore._testCancelPendingSave();
	});

	it("shows File Browser when open and no global workspace is active", async () => {
		uiStore.setFileBrowserPanelVisible(true);
		renderOrchestrator();
		await waitFor(() => expect(h.fileBrowser.length).toBeGreaterThan(0));
		expect(h.fileBrowser[0].visible).toBe(true);
	});

	it("shows Git Panel when open and no global workspace is active", async () => {
		uiStore.setGitPanelVisible(true);
		renderOrchestrator();
		await waitFor(() => expect(h.gitPanel.length).toBeGreaterThan(0));
		expect(h.gitPanel[0].visible).toBe(true);
	});

	it("suppresses File Browser while the manual cross-repo workspace is active", async () => {
		uiStore.setFileBrowserPanelVisible(true);
		globalWorkspaceStore.activate();
		expect(globalWorkspaceStore.getScope()).toBe(MANUAL_SCOPE);

		// Suppressed means never shown, so the code-split panel is never even mounted.
		renderOrchestrator();
		await new Promise((resolve) => setTimeout(resolve, 20));
		expect(h.fileBrowser.length).toBe(0);

		// Leaving the manual workspace lets it mount, visible.
		globalWorkspaceStore.deactivate();
		await waitFor(() => expect(h.fileBrowser.length).toBeGreaterThan(0));
		expect(h.fileBrowser.at(-1)?.visible).toBe(true);
	});

	it("suppresses Git Panel while the manual cross-repo workspace is active", async () => {
		uiStore.setGitPanelVisible(true);
		globalWorkspaceStore.activate();
		expect(globalWorkspaceStore.getScope()).toBe(MANUAL_SCOPE);

		// Suppressed means never shown, so the code-split panel is never even mounted.
		renderOrchestrator();
		await new Promise((resolve) => setTimeout(resolve, 20));
		expect(h.gitPanel.length).toBe(0);

		// Leaving the manual workspace lets it mount, visible.
		globalWorkspaceStore.deactivate();
		await waitFor(() => expect(h.gitPanel.length).toBeGreaterThan(0));
		expect(h.gitPanel.at(-1)?.visible).toBe(true);
	});

	// Regression test for the bug: a repo with its "auto-consolidate worktrees"
	// setting on (#e767) activates the global workspace under its own repo-path
	// scope, not MANUAL_SCOPE. That scope still has one well-defined repo, so
	// File Browser / Git Panel must keep working instead of going dark on every
	// tab of that repo.
	it("does not suppress File Browser when a per-repo consolidated workspace is active", async () => {
		uiStore.setFileBrowserPanelVisible(true);
		globalWorkspaceStore.setScope("/repo/a");
		globalWorkspaceStore.activate();
		expect(globalWorkspaceStore.isActive()).toBe(true);
		expect(globalWorkspaceStore.getScope()).toBe("/repo/a");

		renderOrchestrator();
		await waitFor(() => expect(h.fileBrowser.length).toBeGreaterThan(0));
		expect(h.fileBrowser[0].visible).toBe(true);
	});

	it("does not suppress Git Panel when a per-repo consolidated workspace is active", async () => {
		uiStore.setGitPanelVisible(true);
		globalWorkspaceStore.setScope("/repo/a");
		globalWorkspaceStore.activate();
		expect(globalWorkspaceStore.isActive()).toBe(true);
		expect(globalWorkspaceStore.getScope()).toBe("/repo/a");

		renderOrchestrator();
		await waitFor(() => expect(h.gitPanel.length).toBeGreaterThan(0));
		expect(h.gitPanel[0].visible).toBe(true);
	});

	it("never gates unrelated panels on global workspace state", async () => {
		uiStore.setMarkdownPanelVisible(true);
		globalWorkspaceStore.activate();

		renderOrchestrator();
		await waitFor(() => expect(h.markdown.length).toBeGreaterThan(0));
		expect(h.markdown[0].visible).toBe(true);
	});

	// The panels are code-split: nothing is fetched or mounted until the panel is
	// first shown, and once shown it stays mounted so hide/show keeps its state.
	it("does not mount a side panel until it is first shown, then keeps it mounted", async () => {
		renderOrchestrator();
		await new Promise((resolve) => setTimeout(resolve, 20));
		expect(h.gitPanel.length).toBe(0);
		expect(h.fileBrowser.length).toBe(0);
		expect(h.markdown.length).toBe(0);

		uiStore.setGitPanelVisible(true);
		await waitFor(() => expect(h.gitPanel.length).toBeGreaterThan(0));
		expect(h.gitPanel.at(-1)?.visible).toBe(true);
		expect(h.fileBrowser.length).toBe(0);

		uiStore.setGitPanelVisible(false);
		await waitFor(() => expect(h.gitPanel.at(-1)?.visible).toBe(false));
	});
});
