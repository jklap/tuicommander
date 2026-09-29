import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Story 1263-7d7d: the Log tab indented every commit by the widest lane of the
// whole loaded history, and an expanded commit wrapped its subject, overlapped
// badges, showed a native tooltip and pushed later rows away from their dots.

const h = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("../../invoke", () => ({ invoke: h.invoke }));

import type { GraphNode } from "../../components/GitPanel/CommitGraph";
import { LogTab } from "../../components/GitPanel/LogTab";
import type { CommitLogEntry } from "../../components/GitPanel/types";

const LANE = 16;
const ROW = 48;

const commit = (hash: string, subject: string, extra: Partial<CommitLogEntry> = {}): CommitLogEntry => ({
	hash,
	parents: [],
	refs: [],
	author_name: "Boss",
	author_date: "2026-09-29T10:00:00Z",
	subject,
	...extra,
});

const node = (hash: string, row: number, column: number, connections: GraphNode["connections"] = []): GraphNode => ({
	hash,
	row,
	column,
	color_index: 0,
	parents: [],
	refs: [],
	connections,
});

const conn = (from_col: number, from_row: number, to_col: number, to_row: number) => ({
	from_col,
	from_row,
	to_col,
	to_row,
	color_index: 0,
});

const LONG_SUBJECT = "fix(rb): batch source fingerprints in one process so the gate stops paying the overhead";

const COMMITS = [
	commit("a000000", "first", { refs: ["HEAD -> POC-01016/rb-fingerprint-overhead"] }),
	commit("b000000", LONG_SUBJECT, { body: "Longer explanation." }),
	commit("c000000", "third"),
	commit("d000000", "fourth"),
	commit("e000000", "fifth"),
];

// Row 0: lane 0 only. Rows 1..3: a merge curve from lane 0 to lane 2.
// Row 4: a lone commit on lane 5 — the widest lane of the history.
const NODES = [
	node("a000000", 0, 0, [conn(0, 0, 0, 1)]),
	node("b000000", 1, 0, [conn(0, 1, 0, 2), conn(0, 1, 2, 3)]),
	node("c000000", 2, 0),
	node("d000000", 3, 2),
	node("e000000", 4, 5),
];

/** Dot centres recorded by a fake 2D context: every `arc` call is a dot. */
const dots: { x: number; y: number }[] = [];

class RecordingOffscreenCanvas {
	constructor(
		public width: number,
		public height: number,
	) {}
	getContext() {
		return new Proxy(
			{ arc: (x: number, y: number) => dots.push({ x, y }) },
			{ get: (target, key) => (key in target ? target[key as keyof typeof target] : () => {}) },
		);
	}
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

function rowOf(container: HTMLElement, subject: string): HTMLElement {
	const el = [...container.querySelectorAll<HTMLElement>("[style*='position: absolute']")].find((r) =>
		r.textContent?.includes(subject),
	);
	if (!el) throw new Error(`row "${subject}" not rendered`);
	return el;
}

const padLeft = (row: HTMLElement) => Number.parseFloat(row.style.paddingLeft);

describe("LogTab layout", () => {
	beforeEach(() => {
		dots.length = 0;
		vi.stubGlobal("OffscreenCanvas", RecordingOffscreenCanvas);
		// happy-dom lays nothing out: give the virtualizer a viewport tall
		// enough for every row.
		vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(1000);
		vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(400);
		h.invoke.mockReset();
		h.invoke.mockImplementation((cmd: string) => {
			if (cmd === "get_commit_log") return Promise.resolve(COMMITS);
			if (cmd === "get_commit_graph") return Promise.resolve(NODES);
			return Promise.resolve([]);
		});
	});

	afterEach(() => {
		vi.unstubAllGlobals();
		vi.restoreAllMocks();
	});

	it("indents each row by the lanes drawn in that row, not the widest lane of the history", async () => {
		const { container } = render(() => <LogTab repoPath="/repo" onOpenDiff={vi.fn()} />);
		await settle();

		const pads = ["first", LONG_SUBJECT, "third", "fourth", "fifth"].map((s) => padLeft(rowOf(container, s)));
		// Row 0: one lane. Rows 1..3: the curve reaches lane 2. Row 4: lane 5.
		expect(pads).toEqual([1 * LANE + 4, 3 * LANE + 4, 3 * LANE + 4, 3 * LANE + 4, 6 * LANE + 4]);
	});

	it("keeps the subject on one line without a native tooltip and shows the full message when expanded", async () => {
		const { container } = render(() => <LogTab repoPath="/repo" onOpenDiff={vi.fn()} />);
		await settle();

		const row = rowOf(container, LONG_SUBJECT);
		expect(row.querySelector(`[title="${LONG_SUBJECT}"]`)).toBeNull();

		fireEvent.click(row);
		await settle();

		const expanded = rowOf(container, LONG_SUBJECT);
		expect(expanded.querySelector(`[title="${LONG_SUBJECT}"]`)).toBeNull();
		// Subject once in the header, once in the full message of the body.
		const bodyText = [...expanded.querySelectorAll("div")].map((d) => d.textContent ?? "");
		expect(bodyText.some((t) => t === `${LONG_SUBJECT}\n\nLonger explanation.`)).toBe(true);
	});

	it("keeps every graph dot on its own row's header line after a row expands", async () => {
		const { container } = render(() => <LogTab repoPath="/repo" onOpenDiff={vi.fn()} />);
		await settle();

		fireEvent.click(rowOf(container, LONG_SUBJECT));
		await settle();

		// Oracle: the rendered row's top offset. A dot belongs at the centre of
		// the collapsed-height header of the row that holds its commit.
		const subjects = ["first", LONG_SUBJECT, "third", "fourth", "fifth"];
		const expectedY = subjects.map((s) => Number.parseFloat(rowOf(container, s).style.top) + ROW / 2);
		// The latest rebuild draws the last five dots.
		const drawnY = dots.slice(-NODES.length).map((d) => d.y);
		expect(drawnY).toEqual(expectedY);
		expect(expectedY[2]).toBeGreaterThan(2 * ROW + ROW / 2);
	});
});
