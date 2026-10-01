import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "./mocks/tauri";

// Only the wiring is under test here: the pure color/label helpers are covered by
// FloatingTerminal.statusDerivation.test.ts. This pins that the status pill is sourced
// from `terminalsStore.isWorking` (busy OR declaredBackgroundWork), not the raw `isBusy`.
vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
	emitTo: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
	getCurrentWebviewWindow: () => ({
		onCloseRequested: vi.fn().mockResolvedValue(() => {}),
		setTitle: vi.fn().mockResolvedValue(undefined),
		close: vi.fn().mockResolvedValue(undefined),
	}),
}));

vi.mock("../components/Terminal", () => ({
	Terminal: () => <div data-testid="terminal-stub" />,
}));

vi.mock("../themes", () => ({
	applyAppTheme: vi.fn(),
	applyFontFamily: vi.fn(),
	listenForThemeChanges: vi.fn().mockResolvedValue(undefined),
	loadThemes: vi.fn().mockResolvedValue(undefined),
	themesLoaded: () => false,
}));

import { FloatingTerminal } from "../FloatingTerminal";
import { settingsStore } from "../stores/settings";
import { terminalsStore } from "../stores/terminals";

const TAB_ID = "float-tab-1";

describe("FloatingTerminal status pill wiring", () => {
	beforeEach(() => {
		window.location.hash = `#/floating?sessionId=sess-float&tabId=${TAB_ID}&name=Float`;
		vi.spyOn(settingsStore, "hydrate").mockResolvedValue(undefined as never);
	});

	afterEach(() => {
		cleanup();
		terminalsStore.remove(TAB_ID);
		vi.restoreAllMocks();
		window.location.hash = "";
	});

	async function mountReady() {
		const utils = render(() => <FloatingTerminal />);
		// `ready()` flips after the async onMount (hydrate, loadThemes, register).
		await waitFor(() => {
			expect(terminalsStore.get(TAB_ID)).toBeDefined();
			expect(utils.container.textContent).toContain("●");
		});
		return utils;
	}

	it("shows Idle for an idle shell with no declared background work", async () => {
		const { container } = await mountReady();
		terminalsStore.update(TAB_ID, { shellState: "idle", declaredBackgroundWork: false });

		await waitFor(() => {
			expect(container.textContent).toContain("Idle");
		});
		expect(container.textContent).not.toContain("Running");
	});

	it("shows Running for an idle shell when the terminal has declared background work", async () => {
		const { container } = await mountReady();
		terminalsStore.update(TAB_ID, { shellState: "idle", declaredBackgroundWork: true });

		await waitFor(() => {
			expect(container.textContent).toContain("Running");
		});
		expect(terminalsStore.isBusy(TAB_ID)).toBe(false);
		expect(container.textContent).not.toContain("Idle");
	});

	it("drops back to Idle when the declared background work is cleared", async () => {
		const { container } = await mountReady();
		terminalsStore.update(TAB_ID, { shellState: "idle", declaredBackgroundWork: true });
		await waitFor(() => {
			expect(container.textContent).toContain("Running");
		});

		terminalsStore.update(TAB_ID, { declaredBackgroundWork: false });
		await waitFor(() => {
			expect(container.textContent).toContain("Idle");
		});
	});
});
