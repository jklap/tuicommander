import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { terminalsStore } from "../../../stores/terminals";
import { TerminalTabView } from "../TabViews";

afterEach(cleanup);

function addTerminal(overrides: Partial<Parameters<typeof terminalsStore.update>[1]> = {}) {
	const id = terminalsStore.add({
		sessionId: null,
		fontSize: 14,
		name: "Test",
		cwd: null,
		awaitingInput: null,
	});
	if (Object.keys(overrides).length > 0) terminalsStore.update(id, overrides);
	return id;
}

function renderTab(id: string) {
	return render(() => (
		<TerminalTabView
			id={id}
			index={0}
			paneRects={[]}
			isDragging={false}
			isDragOver={false}
			dragOverSide={null}
			dragInvalid={false}
			quickSwitcherActive={false}
			isEditing={false}
			showWorkspaceMetadata={false}
			onSelect={() => {}}
			onClose={() => {}}
			onContextMenu={() => {}}
			onPointerDown={() => {}}
			onStartEditing={() => {}}
			onCommitRename={() => {}}
		/>
	));
}

describe("TerminalTabView progress bar", () => {
	afterEach(() => {
		for (const id of Object.keys(terminalsStore.state.terminals)) terminalsStore.remove(id);
	});

	it("renders nothing when progress is null", () => {
		const id = addTerminal();
		const { container } = renderTab(id);
		expect(container.querySelector('[data-tab-id] > [class*="progress"]')).toBeNull();
	});

	it("shows an indeterminate progress bar with no inline transform", () => {
		const id = addTerminal({ progress: { kind: "indeterminate", value: 90 } });
		const { container } = renderTab(id);
		const bar = container.querySelector(`[data-tab-id="${id}"] [data-kind]`) as HTMLElement;
		expect(bar).not.toBeNull();
		expect(bar.getAttribute("data-kind")).toBe("indeterminate");
		// The indeterminate CSS rule (TabBar.module.css) owns the sweep animation via
		// background-position — an inline transform here would fight/override it.
		expect(bar.style.transform).toBe("");
	});

	it("scales a normal progress bar by value/100 via inline transform", () => {
		const id = addTerminal({ progress: { kind: "normal", value: 40 } });
		const { container } = renderTab(id);
		const bar = container.querySelector(`[data-tab-id="${id}"] [data-kind]`) as HTMLElement;
		expect(bar.getAttribute("data-kind")).toBe("normal");
		expect(bar.style.transform).toBe("scaleX(0.4)");
	});

	it("defaults to a full-width transform when value is null for a determinate kind", () => {
		const id = addTerminal({ progress: { kind: "error", value: null } });
		const { container } = renderTab(id);
		const bar = container.querySelector(`[data-tab-id="${id}"] [data-kind]`) as HTMLElement;
		expect(bar.getAttribute("data-kind")).toBe("error");
		expect(bar.style.transform).toBe("scaleX(1)");
	});
});

describe("TerminalTabView isRemote badge/class", () => {
	afterEach(() => {
		for (const id of Object.keys(terminalsStore.state.terminals)) terminalsStore.remove(id);
	});

	it("shows the 'PTY · ' prefix only when isRemote and agentType are both set", () => {
		const id = addTerminal({ isRemote: true, agentType: "claude" });
		const { container } = renderTab(id);
		const tabName = container.querySelector(`[data-tab-id="${id}"] .tabName`) ?? container.querySelector(".tabName");
		expect(tabName?.textContent).toContain("PTY ·");
	});

	it("does not show the 'PTY · ' prefix when isRemote is true but there is no agentType", () => {
		const id = addTerminal({ isRemote: true, agentType: null });
		const { container } = renderTab(id);
		const tabName = container.querySelector(".tabName");
		expect(tabName?.textContent).not.toContain("PTY ·");
	});

	it("does not show the 'PTY · ' prefix for a non-remote tab even with an agentType", () => {
		const id = addTerminal({ isRemote: false, agentType: "claude" });
		const { container } = renderTab(id);
		const tabName = container.querySelector(".tabName");
		expect(tabName?.textContent).not.toContain("PTY ·");
	});

	it("applies the remoteTab class when isRemote is true", () => {
		const id = addTerminal({ isRemote: true });
		const { container } = renderTab(id);
		const tab = container.querySelector(`[data-tab-id="${id}"]`) as HTMLElement;
		expect(Array.from(tab.classList).some((c) => c.toLowerCase().includes("remotetab"))).toBe(true);
	});

	it("does not apply the remoteTab class when isRemote is false", () => {
		const id = addTerminal({ isRemote: false });
		const { container } = renderTab(id);
		const tab = container.querySelector(`[data-tab-id="${id}"]`) as HTMLElement;
		expect(Array.from(tab.classList).some((c) => c.toLowerCase().includes("remotetab"))).toBe(false);
	});
});

describe("TerminalTabView busy/idle dot", () => {
	afterEach(() => {
		for (const id of Object.keys(terminalsStore.state.terminals)) terminalsStore.remove(id);
	});

	it("applies the busy class when the terminal has declared background work despite an idle shell", () => {
		const id = addTerminal({ shellState: "idle", declaredBackgroundWork: true });
		const { container } = renderTab(id);
		const tab = container.querySelector(`[data-tab-id="${id}"]`);
		expect(tab?.className).toMatch(/shellBusy/);
		expect(tab?.className).not.toMatch(/shellIdle/);
	});

	it("applies the idle class (not busy) for a plain idle terminal with no declared background work", () => {
		const id = addTerminal({ shellState: "idle" });
		const { container } = renderTab(id);
		const tab = container.querySelector(`[data-tab-id="${id}"]`);
		expect(tab?.className).toMatch(/shellIdle/);
		expect(tab?.className).not.toMatch(/shellBusy/);
	});

	it("applies the busy class for a plain busy shell (no declared background work involved)", () => {
		const id = addTerminal({ shellState: "busy" });
		const { container } = renderTab(id);
		const tab = container.querySelector(`[data-tab-id="${id}"]`);
		expect(tab?.className).toMatch(/shellBusy/);
	});
});
