import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
	effectiveActivityState as EffectiveStateFn,
	terminalStatusLabel as LabelFn,
	reconcileActivityOrder as ReconcileFn,
} from "../../utils/activitySnapshot";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
}));

describe("activitySnapshot", () => {
	let buildActivitySnapshot: typeof import("../../utils/activitySnapshot").buildActivitySnapshot;
	let snapshotToRows: typeof import("../../panelAdapters/activity").snapshotToRows;
	let terminalsStore: typeof import("../../stores/terminals").terminalsStore;
	let globalWorkspaceStore: typeof import("../../stores/globalWorkspace").globalWorkspaceStore;

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

		const termMod = await import("../../stores/terminals");
		terminalsStore = termMod.terminalsStore;
		const gwMod = await import("../../stores/globalWorkspace");
		globalWorkspaceStore = gwMod.globalWorkspaceStore;
		const snapMod = await import("../../utils/activitySnapshot");
		buildActivitySnapshot = snapMod.buildActivitySnapshot;
		snapshotToRows = (await import("../../panelAdapters/activity")).snapshotToRows;
	});

	it("returns empty terminals array when none exist", () => {
		const snap = buildActivitySnapshot();
		expect(snap.terminals).toEqual([]);
	});

	it("includes all terminal fields in snapshot", () => {
		const id = terminalsStore.add({
			name: "Terminal 1",
			sessionId: "sess1",
			cwd: "/Users/test/project",
			fontSize: 14,
			awaitingInput: null,
			agentType: "claude",
		});
		terminalsStore.update(id, {
			shellState: "busy",
			agentIntent: "Writing tests",
			lastPrompt: "Write tests for panelSync",
		});

		const snap = buildActivitySnapshot();
		expect(snap.terminals).toHaveLength(1);

		const t = snap.terminals[0];
		expect(t.id).toBe(id);
		expect(t.name).toBe("Terminal 1");
		expect(t.shellState).toBe("busy");
		expect(t.awaitingInput).toBeNull();
		expect(t.sessionId).toBe("sess1");
		expect(t.agentType).toBe("claude");
		expect(t.agentIntent).toBe("Writing tests");
		expect(t.currentTask).toBeNull(); // claude agentType suppresses currentTask
		expect(t.lastPrompt).toBe("Write tests for panelSync");
		expect(t.cwd).toBe("/Users/test/project");
		expect(typeof t.isActive).toBe("boolean");
		expect(typeof t.isRateLimited).toBe("boolean");
		expect(typeof t.isPromoted).toBe("boolean");
	});

	it("shows currentTask for non-claude agents", () => {
		const id = terminalsStore.add({
			name: "Terminal 2",
			sessionId: "sess2",
			cwd: null,
			fontSize: 14,
			awaitingInput: null,
			agentType: "aider",
		});
		terminalsStore.update(id, { currentTask: "Running migration" });

		const snap = buildActivitySnapshot();
		expect(snap.terminals[0].currentTask).toBe("Running migration");
	});

	it("reflects isPromoted from globalWorkspaceStore", () => {
		const id = terminalsStore.add({
			name: "Terminal 3",
			sessionId: null,
			cwd: null,
			fontSize: 14,
			awaitingInput: null,
		});
		globalWorkspaceStore.togglePromote(id);

		const snap = buildActivitySnapshot();
		expect(snap.terminals[0].isPromoted).toBe(true);
	});

	it("keeps completed snapshot rows idle-styled despite a stale busy debounce", () => {
		const id = terminalsStore.add({
			name: "Terminal 4",
			sessionId: "sess4",
			cwd: null,
			fontSize: 14,
			awaitingInput: null,
			agentType: "codex",
		});
		terminalsStore.update(id, { shellState: "busy", agentState: "completed", backgroundWork: false });

		const row = snapshotToRows(buildActivitySnapshot())[0];
		expect(row.status.label).toBe("Completed");
		expect(row.isWorking).toBe(false);
	});

	it("orders the snapshot working-first, idle-second", () => {
		const idleId = terminalsStore.add({
			name: "Idle terminal",
			sessionId: "sess-idle",
			cwd: null,
			fontSize: 14,
			awaitingInput: null,
		});
		terminalsStore.update(idleId, { shellState: "idle" });
		const busyId = terminalsStore.add({
			name: "Busy terminal",
			sessionId: "sess-busy",
			cwd: null,
			fontSize: 14,
			awaitingInput: null,
		});
		terminalsStore.update(busyId, { shellState: "busy" });

		const snap = buildActivitySnapshot();
		expect(snap.terminals.map((t) => t.id)).toEqual([busyId, idleId]);
	});

	it("orders the idle group by idleSince descending (most-recently-active first)", () => {
		// A direct null→idle transition (never having been busy) sets idleSince without
		// starting the busy→idle cooldown timer (see handleShellStateChange), so this
		// avoids leaving a dangling setTimeout behind when the test ends.
		const olderId = terminalsStore.add({
			name: "Older idle",
			sessionId: "sess-older",
			cwd: null,
			fontSize: 14,
			awaitingInput: null,
		});
		terminalsStore.update(olderId, { shellState: "idle" });

		const newerId = terminalsStore.add({
			name: "Newer idle",
			sessionId: "sess-newer",
			cwd: null,
			fontSize: 14,
			awaitingInput: null,
		});
		terminalsStore.update(newerId, { shellState: "idle" });
		// Force the second terminal's idleSince strictly later than the first's, since
		// both may otherwise land on the same millisecond in a fast test run.
		terminalsStore.update(newerId, { idleSince: (terminalsStore.get(olderId)?.idleSince ?? 0) + 1000 });

		const snap = buildActivitySnapshot();
		expect(snap.terminals.map((t) => t.id)).toEqual([newerId, olderId]);
	});

	it("renders an omitted busy session as exited rather than working", () => {
		const row = snapshotToRows({
			terminals: [
				{
					id: "omitted-session",
					name: "Terminal 5",
					shellState: "exited",
					awaitingInput: null,
					sessionId: null,
					agentType: "claude",
					agentIntent: null,
					currentTask: null,
					lastPrompt: null,
					activeSubTasks: 0,
					cwd: null,
					lastDataAt: null,
					idleSince: null,
					isActive: false,
					isRateLimited: false,
					agentState: null,
					backgroundWork: false,
					isBusy: true,
					isPromoted: false,
				},
			],
		})[0];
		expect(row.status.label).toBe("—");
		expect(row.isWorking).toBe(false);
	});
});

describe("terminalStatusLabel", () => {
	let terminalStatusLabel: typeof LabelFn;
	let effectiveActivityState: typeof EffectiveStateFn;
	beforeEach(async () => {
		vi.resetModules();
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		terminalStatusLabel = (await import("../../utils/activitySnapshot")).terminalStatusLabel;
		effectiveActivityState = (await import("../../utils/activitySnapshot")).effectiveActivityState;
	});

	const cls = { rateLimited: "RL", error: "ERR", waiting: "WAIT", working: "WORK", idle: "IDLE" };

	it("rate-limited wins over everything", () => {
		expect(terminalStatusLabel("busy", "error", true, cls)).toEqual({ label: "Rate limited", className: "RL" });
	});

	it("labels an API error as Error, NOT Waiting for input", () => {
		// Regression: an errored agent must not be collapsed into "Waiting for input".
		expect(terminalStatusLabel("idle", "error", false, cls)).toEqual({ label: "Error", className: "ERR" });
	});

	it("labels a question as Waiting for input", () => {
		expect(terminalStatusLabel("idle", "question", false, cls)).toEqual({
			label: "Waiting for input",
			className: "WAIT",
		});
	});

	it("maps shellState busy/idle when no awaiting input", () => {
		expect(terminalStatusLabel("busy", null, false, cls)).toEqual({ label: "Working", className: "WORK" });
		expect(terminalStatusLabel("idle", null, false, cls)).toEqual({ label: "Idle", className: "IDLE" });
		expect(terminalStatusLabel(null, null, false, cls)).toEqual({ label: "—", className: "IDLE" });
	});

	it("shows a ready composer as idle even when Codex retains a background terminal", () => {
		expect(effectiveActivityState("idle", null, false, "working", true)).toBe("idle");
		expect(terminalStatusLabel("idle", null, false, cls, "working", true)).toEqual({
			label: "Idle",
			className: "IDLE",
		});
	});

	it("keeps background work authoritative until the composer is ready", () => {
		expect(effectiveActivityState("busy", null, false, "working", true)).toBe("working");
		expect(effectiveActivityState(null, null, false, "working", true)).toBe("working");
	});

	it("preserves completed instead of reviving stale shell activity", () => {
		expect(effectiveActivityState("busy", null, false, "completed", false)).toBe("completed");
		expect(terminalStatusLabel("busy", null, false, cls, "completed", false)).toEqual({
			label: "Completed",
			className: "IDLE",
		});
	});

	it("lets live shell activity override a lagging idle lifecycle snapshot", () => {
		expect(effectiveActivityState("busy", null, false, "idle", false)).toBe("working");
		expect(terminalStatusLabel("busy", null, false, cls, "idle", false)).toEqual({
			label: "Working",
			className: "WORK",
		});
	});

	it("uses lifecycle awaiting-input when the parsed frontend event is stale or absent", () => {
		expect(effectiveActivityState("idle", null, false, "awaiting_input", false)).toBe("awaiting_input");
		expect(terminalStatusLabel("idle", null, false, cls, "awaiting_input", false)).toEqual({
			label: "Waiting for input",
			className: "WAIT",
		});
	});

	it("lets a fresh idle lifecycle clear a prior working lifecycle", () => {
		expect(effectiveActivityState("busy", null, false, "working", true)).toBe("working");
		expect(effectiveActivityState("idle", null, false, "idle", false)).toBe("idle");
	});
});

describe("reconcileActivityOrder", () => {
	let reconcileActivityOrder: typeof ReconcileFn;
	beforeEach(async () => {
		vi.resetModules();
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		reconcileActivityOrder = (await import("../../utils/activitySnapshot")).reconcileActivityOrder;
	});

	const working = (set: Set<string>) => (id: string) => set.has(id);

	it("partitions working-first, idle-second, each in first-seen order", () => {
		const spine: string[] = [];
		const order = reconcileActivityOrder(spine, ["a", "b", "c", "d"], working(new Set(["b", "d"])));
		expect(order).toEqual(["b", "d", "a", "c"]);
	});

	it("keeps a terminal in place while its working state is unchanged", () => {
		const spine: string[] = [];
		const w = new Set(["a", "b"]);
		const first = reconcileActivityOrder(spine, ["a", "b", "c"], working(w));
		// Recompute with the SAME states — order must be identical (no avanti-e-indietro).
		const second = reconcileActivityOrder(spine, ["a", "b", "c"], working(w));
		expect(second).toEqual(first);
	});

	it("moves a terminal only when it crosses the working/idle boundary", () => {
		const spine: string[] = [];
		reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set(["a"])));
		// b flips to working — it joins the working group at its spine position.
		const after = reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set(["a", "b"])));
		expect(after).toEqual(["a", "b", "c"]);
	});

	it("appends newly-seen terminals at the end of their group", () => {
		const spine: string[] = [];
		reconcileActivityOrder(spine, ["a", "b"], working(new Set(["a"])));
		const after = reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set(["a", "c"])));
		// c is new + working → after existing working 'a'; idle 'b' stays last.
		expect(after).toEqual(["a", "c", "b"]);
	});

	it("drops removed terminals while preserving relative order", () => {
		const spine: string[] = [];
		reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set()));
		const after = reconcileActivityOrder(spine, ["a", "c"], working(new Set()));
		expect(after).toEqual(["a", "c"]);
		expect(spine).toEqual(["a", "c"]);
	});

	it("appends a terminal at the end (not its old slot) if it disappears and comes back", () => {
		const spine: string[] = [];
		reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set()));
		// b drops out...
		reconcileActivityOrder(spine, ["a", "c"], working(new Set()));
		// ...then reappears. It re-enters at the end of the spine, not back at index 1.
		const after = reconcileActivityOrder(spine, ["a", "c", "b"], working(new Set()));
		expect(after).toEqual(["a", "c", "b"]);
	});

	describe("idleSortKey", () => {
		const keyOf = (keys: Record<string, number | null>) => (id: string) => keys[id] ?? null;

		it("sorts the idle group by key descending (most-recently-active first)", () => {
			const spine: string[] = [];
			const order = reconcileActivityOrder(
				spine,
				["a", "b", "c"],
				working(new Set()),
				keyOf({ a: 100, b: 300, c: 200 }),
			);
			expect(order).toEqual(["b", "c", "a"]);
		});

		it("never sorts the working group", () => {
			const spine: string[] = [];
			// Working keys would sort b before a if the working group were sorted too.
			const order = reconcileActivityOrder(
				spine,
				["a", "b", "c"],
				working(new Set(["a", "b"])),
				keyOf({ a: 1, b: 999, c: 500 }),
			);
			expect(order).toEqual(["a", "b", "c"]);
		});

		it("places null keys last", () => {
			const spine: string[] = [];
			const order = reconcileActivityOrder(
				spine,
				["a", "b", "c"],
				working(new Set()),
				keyOf({ a: 100, b: null as unknown as number, c: 200 }),
			);
			expect(order).toEqual(["c", "a", "b"]);
		});

		it("keeps spine order for equal keys (stable sort)", () => {
			const spine: string[] = [];
			reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set()));
			const order = reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set()), keyOf({ a: 1, b: 1, c: 1 }));
			expect(order).toEqual(["a", "b", "c"]);
		});

		it("does not oscillate on repeated calls with unchanged keys", () => {
			const spine: string[] = [];
			const key = keyOf({ a: 100, b: 300, c: 200 });
			const first = reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set()), key);
			const second = reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set()), key);
			expect(second).toEqual(first);
		});

		it("moves a row out of the sorted group entirely once it starts working", () => {
			const spine: string[] = [];
			const key = keyOf({ a: 100, b: 300, c: 200 });
			reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set()), key);
			// b (highest idle key) starts working — it leaves the sorted idle group and
			// heads the working group instead, regardless of its old idle key.
			const after = reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set(["b"])), key);
			expect(after).toEqual(["b", "c", "a"]);
		});

		it("omitting the parameter reproduces the pre-Phase-1 spine-order behavior", () => {
			const spine: string[] = [];
			const order = reconcileActivityOrder(spine, ["a", "b", "c", "d"], working(new Set(["b", "d"])));
			expect(order).toEqual(["b", "d", "a", "c"]);
		});
	});
});

describe("displayTask", () => {
	let displayTask: typeof import("../../utils/activitySnapshot").displayTask;

	beforeEach(async () => {
		vi.resetModules();
		displayTask = (await import("../../utils/activitySnapshot")).displayTask;
	});

	it("drops a task that only restates the status badge next to it", () => {
		// "Working / Working" costs a line and carries nothing.
		expect(displayTask("Working", "codex")).toBeNull();
		expect(displayTask("working", "codex")).toBeNull();
		expect(displayTask("  Idle  ", "codex")).toBeNull();
		expect(displayTask("Waiting for input", "codex")).toBeNull();
		expect(displayTask("Rate limited", "codex")).toBeNull();
	});

	it("drops every Claude verb, matching the badge wording or not", () => {
		// Claude cycles decorative spinner words that mean "working" without
		// saying it, so no exact match can catch them.
		expect(displayTask("Undulating", "claude")).toBeNull();
		expect(displayTask("Crunching", "claude")).toBeNull();
		expect(displayTask("Reading files", "claude")).toBeNull();
	});

	it("keeps a real task that merely starts like a status word", () => {
		// The match is exact: this one says something the badge does not.
		expect(displayTask("Waiting for background terminal", "codex")).toBe("Waiting for background terminal");
		expect(displayTask("Working on the parser", "codex")).toBe("Working on the parser");
	});

	it("has nothing to show when the agent reported no task", () => {
		expect(displayTask(null, "codex")).toBeNull();
		expect(displayTask(undefined, "codex")).toBeNull();
		expect(displayTask("", "codex")).toBeNull();
	});

	it("applies the same rule to a session with no known agent", () => {
		expect(displayTask("Working", null)).toBeNull();
		expect(displayTask("Waiting for background terminal", null)).toBe("Waiting for background terminal");
	});
});
