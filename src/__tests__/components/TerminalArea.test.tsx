import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

// The real Terminal needs a live PTY/xterm; this file only tests which layer
// (split vs. full-page orphan) TerminalArea shows.
vi.mock("../../components/Terminal", () => ({
	Terminal: () => null,
}));

import { TerminalArea } from "../../components/TerminalArea";
import { diffTabsStore } from "../../stores/diffTabs";
import { paneLayoutStore, resetGroupCounter } from "../../stores/paneLayout";
import { terminalsStore } from "../../stores/terminals";

const addTerminal = (name: string) =>
	terminalsStore.add({ sessionId: null, fontSize: 14, name, cwd: null, awaitingInput: null });

function renderArea() {
	return render(() => (
		<TerminalArea onTerminalFocus={vi.fn()} onCloseTab={vi.fn()} onOpenFilePath={vi.fn()} onContextMenu={vi.fn()} />
	));
}

/** The wrapper TerminalArea puts around the PaneTree; its visibility is what hides a split behind an orphan. */
const splitWrapper = (container: HTMLElement) =>
	container.querySelector<HTMLElement>('#terminal-panes > div[style*="display: contents"]');

describe("TerminalArea split/orphan overlay", () => {
	beforeEach(() => {
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		diffTabsStore.clearAll();
		paneLayoutStore.reset();
		resetGroupCounter();
	});
	afterEach(() => {
		paneLayoutStore._testCancelPendingSave();
	});

	const agentSplit = () => {
		const a = addTerminal("agent-a");
		const b = addTerminal("agent-b");
		paneLayoutStore.arrangeSessionsAsLayout([a, b], "tiled");
		return { a, b };
	};

	it("shows the split when the active terminal belongs to it", () => {
		const { a } = agentSplit();
		terminalsStore.setActive(a);

		const { container } = renderArea();

		expect(splitWrapper(container)?.style.visibility).toBe("visible");
	});

	it("hides the split behind an active terminal that is in no pane group", () => {
		agentSplit();
		terminalsStore.setActive(addTerminal("main 1"));

		const { container } = renderArea();

		expect(splitWrapper(container)?.style.visibility).toBe("hidden");
	});

	it("hides the split behind an active orphan diff tab", () => {
		agentSplit();
		diffTabsStore.setActive(diffTabsStore.add("/repo", "file.ts", "M"));

		const { container } = renderArea();

		expect(splitWrapper(container)?.style.visibility).toBe("hidden");
	});

	it("reveals the split again once the orphan is gone", () => {
		const { a } = agentSplit();
		const main = addTerminal("main 1");
		terminalsStore.setActive(main);
		const { container } = renderArea();
		expect(splitWrapper(container)?.style.visibility).toBe("hidden");

		terminalsStore.remove(main);
		terminalsStore.setActive(a);

		expect(splitWrapper(container)?.style.visibility).toBe("visible");
	});

	it("renders no split wrapper when there is no split", () => {
		terminalsStore.setActive(addTerminal("main 1"));

		const { container } = renderArea();

		expect(splitWrapper(container)).toBeNull();
	});
});
