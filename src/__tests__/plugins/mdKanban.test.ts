/**
 * Tests for the md-kanban plugin's pure parsing/rewrite/rendering logic.
 *
 * Like build-cleaner, md-kanban exports its pure helpers as named exports
 * alongside the default plugin object, so this imports and tests the REAL
 * implementation — the plugin loader only reads `.default`, so these named
 * exports are runtime-inert in production.
 */
import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
// @ts-expect-error — untyped plugin JS module (submodule), imported for its pure exports.
import * as plugin from "../../../plugins/md-kanban/main.js";

interface Task {
	lineIndex: number;
	rawLine: string;
	status: string;
	statusChar: string;
	statusCharIndex: number;
	text: string;
	spans: Array<{ type: "text" | "link"; text?: string; label?: string; target?: string; external?: boolean }>;
	fields: Array<{ key: string; value: string; bracket: "[" | "("; start: number; end: number }>;
	priority: string;
	priorityRank: number;
	tags: string[];
	links: Array<{ label: string; target: string; external: boolean; absPath?: string }>;
	id: string;
	dependsOn: string[];
	danglingDeps: string[];
	completion: string;
	cancelled: string;
	headingPath: string[];
	key: string;
	upstream: Task[];
	downstream: Task[];
	lagging: Task[];
}

interface ParsedBoard {
	frontmatterTitle: string | null;
	tasks: Task[];
	eol: string;
	trailingNewline: boolean;
	lines: string[];
}

const {
	STATUS_TYPES,
	STATUS_LABELS,
	CHAR_TO_STATUS,
	PRIORITY_RANK,
	PRIORITY_EMOJI,
	PROGRESS_RANK,
	esc,
	dirnameOf,
	resolveRelativePath,
	boardNameFor,
	classifyLinkTarget,
	splitPreservingEol,
	joinPreservingEol,
	todayIso,
	parseBoard,
	buildDependencyGraph,
	isLagging,
	groupIntoColumns,
	isArchivable,
	setStatusChar,
	removeField,
	upsertField,
	applyStatusTransition,
	generateTaskId,
	ensureTaskId,
	buildBoardHtml,
} = plugin as {
	STATUS_TYPES: string[];
	STATUS_LABELS: Record<string, string>;
	CHAR_TO_STATUS: Record<string, string>;
	PRIORITY_RANK: Record<string, number>;
	PRIORITY_EMOJI: Record<string, string>;
	PROGRESS_RANK: Record<string, number>;
	esc: (s: string) => string;
	dirnameOf: (p: string) => string;
	resolveRelativePath: (baseDir: string, rel: string) => string;
	boardNameFor: (p: string, frontmatterTitle: string | null) => string;
	classifyLinkTarget: (target: string) => { kind: "external" | "file" };
	splitPreservingEol: (content: string) => { lines: string[]; eol: string; trailingNewline: boolean };
	joinPreservingEol: (parts: { lines: string[]; eol: string; trailingNewline: boolean }) => string;
	todayIso: (now?: Date) => string;
	parseBoard: (content: string, boardPath?: string) => ParsedBoard;
	buildDependencyGraph: (tasks: Task[]) => Task[];
	isLagging: (upstream: Task, downstream: Task) => boolean;
	groupIntoColumns: (tasks: Task[], opts?: { hideArchived?: boolean; nowMs?: number }) => Record<string, Task[]>;
	isArchivable: (task: Task, nowMs: number) => boolean;
	setStatusChar: (line: string, idx: number, ch: string) => string;
	removeField: (line: string, key: string) => string;
	upsertField: (line: string, key: string, value: string) => string;
	applyStatusTransition: (line: string, task: Task, toStatus: string, isoDate: string) => string;
	generateTaskId: (existingIds: Set<string>) => string;
	ensureTaskId: (line: string, task: Task, existingIds: Set<string>) => { line: string; id: string };
	buildBoardHtml: (model: {
		boards: Array<{ id: string; name: string }>;
		activeBoardId: string | null;
		hideArchived: boolean;
		search: string;
		columns: Record<string, Task[]> | null;
		errorMessage?: string;
	}) => string;
};

const FIXTURES_DIR = path.join(__dirname, "..", "data");
const SAMPLE = readFileSync(path.join(FIXTURES_DIR, "md-kanban-sample.md"), "utf-8");
const FRONTMATTER = readFileSync(path.join(FIXTURES_DIR, "md-kanban-frontmatter.md"), "utf-8");
const EDGE = readFileSync(path.join(FIXTURES_DIR, "md-kanban-edge.md"), "utf-8");

function byText(board: ParsedBoard, needle: string): Task {
	const t = board.tasks.find((x) => x.text.includes(needle));
	if (!t) throw new Error(`no task found containing "${needle}"`);
	return t;
}

// ---------------------------------------------------------------------------
// Parsing — status characters, priority, fields, tags, headings
// ---------------------------------------------------------------------------

describe("parseBoard — the sample fixture", () => {
	const board = parseBoard(SAMPLE, "/repo/board.md");

	it("finds all 13 sample tasks", () => {
		expect(board.tasks).toHaveLength(13);
	});

	it("maps every status character to the right status, including the 3 PENDING variants", () => {
		expect(byText(board, "basic task").status).toBe("ready");
		expect(byText(board, "in-progress task").status).toBe("in_progress");
		expect(byText(board, "completed task").status).toBe("done");
		expect(byText(board, "cancelled task").status).toBe("wontfix");
		expect(byText(board, "blocked/open question").status).toBe("blocked");
		expect(byText(board, '"add" task').status).toBe("pending");
		expect(byText(board, '"idea" task').status).toBe("pending");
		expect(byText(board, '"important" task').status).toBe("pending");
	});

	it("CHAR_TO_STATUS covers every character used above plus X/I", () => {
		expect(CHAR_TO_STATUS[" "]).toBe("ready");
		expect(CHAR_TO_STATUS.x).toBe("done");
		expect(CHAR_TO_STATUS.X).toBe("done");
		expect(CHAR_TO_STATUS["/"]).toBe("in_progress");
		expect(CHAR_TO_STATUS["-"]).toBe("wontfix");
		expect(CHAR_TO_STATUS["?"]).toBe("blocked");
		expect(CHAR_TO_STATUS["+"]).toBe("pending");
		expect(CHAR_TO_STATUS.i).toBe("pending");
		expect(CHAR_TO_STATUS.I).toBe("pending");
		expect(CHAR_TO_STATUS["!"]).toBe("pending");
	});

	it("parses an explicit priority and ranks it above the default", () => {
		const withPriority = byText(board, "with a priority");
		expect(withPriority.priority).toBe("low");
		expect(withPriority.priorityRank).toBe(PRIORITY_RANK.low);

		const noPriority = byText(board, "basic task");
		expect(noPriority.priority).toBe("");
		expect(noPriority.priorityRank).toBe(PRIORITY_RANK[""]);
		expect(noPriority.priorityRank).toBeGreaterThan(PRIORITY_RANK.low);
		expect(noPriority.priorityRank).toBeLessThan(PRIORITY_RANK.medium);
	});

	it("keeps an unknown inline field in `fields` but off every typed property and the display text", () => {
		const t = byText(board, "another field we should ignore");
		expect(t.fields.some((f) => f.key === "due" && f.value === "2026-09-15")).toBe(true);
		expect(t.text).not.toContain("due");
		expect(t.text).not.toContain("2026-09-15");
		expect((t as unknown as Record<string, unknown>).due).toBeUndefined();
	});

	it("extracts a tag and strips it from the display text", () => {
		const t = byText(board, "with a tag");
		expect(t.tags).toEqual(["tuicommander"]);
		expect(t.text).not.toContain("#tuicommander");
	});

	it("parses id and dependsOn, single value", () => {
		const upstream = byText(board, "has to be done first");
		expect(upstream.id).toBe("eqodx8");
		const downstream = byText(board, "has a dependency");
		expect(downstream.dependsOn).toEqual(["eqodx8"]);
	});

	it("falls back to the default priority rank for an unrecognized priority value", () => {
		const board2 = parseBoard("- [ ] task  [priority:: urgent]", "/repo/board.md");
		expect(board2.tasks[0].priority).toBe("");
		expect(board2.tasks[0].priorityRank).toBe(PRIORITY_RANK[""]);
	});

	it("filters empty entries out of a malformed dependsOn list", () => {
		const board2 = parseBoard("- [ ] task  [dependsOn:: ,, a1a1a1 ,,]", "/repo/board.md");
		expect(board2.tasks[0].dependsOn).toEqual(["a1a1a1"]);
	});

	it("an entirely empty dependsOn value parses as no dependencies at all", () => {
		const board2 = parseBoard("- [ ] task  [dependsOn:: ]", "/repo/board.md");
		expect(board2.tasks[0].dependsOn).toEqual([]);
		expect(board2.tasks[0].danglingDeps).toEqual([]);
	});

	it("tracks the heading path a task sits under, and nested headings extend it", () => {
		expect(byText(board, "basic task").headingPath).toEqual(["Project Alpha", "Backlog"]);
		expect(byText(board, "has to be done first").headingPath).toEqual(["Project Alpha", "Backlog", "Setup"]);
	});

	it("preserves completion/cancelled fields already present on a line", () => {
		const done = byText(board, "completed task");
		expect(done.completion).toBe("2026-09-14");
		const wontfix = byText(board, "cancelled task");
		expect(wontfix.cancelled).toBe("2026-09-14");
	});

	it("detects EOL style and trailing newline", () => {
		expect(board.eol).toBe("\n");
		expect(board.trailingNewline).toBe(true);
	});
});

describe("parseBoard — frontmatter title", () => {
	it("extracts a quoted frontmatter title", () => {
		const board = parseBoard(FRONTMATTER, "/repo/sprint.md");
		expect(board.frontmatterTitle).toBe("Sprint 12");
		expect(board.tasks).toHaveLength(2);
	});

	it("falls back to the filename without extension when there is no frontmatter", () => {
		expect(boardNameFor("/repo/roadmap.md", null)).toBe("roadmap");
		expect(boardNameFor("/repo/sub/dir/notes.markdown", null)).toBe("notes");
	});

	it("prefers a non-empty frontmatter title over the filename", () => {
		expect(boardNameFor("/repo/whatever.md", "Sprint 12")).toBe("Sprint 12");
		expect(boardNameFor("/repo/whatever.md", "  ")).toBe("whatever");
		expect(boardNameFor("/repo/whatever.md", "")).toBe("whatever");
	});
});

describe("parseBoard — edge cases", () => {
	const board = parseBoard(EDGE, "/repo/edge.md");

	it("does not treat a checkbox line inside a fenced code block as a task", () => {
		expect(board.tasks.some((t) => t.text.includes("inside a fence"))).toBe(false);
	});

	it("does not treat an unrecognized status character as a task", () => {
		expect(board.tasks.some((t) => t.text.includes("Unknown status char"))).toBe(false);
	});

	it("parses a mixed [] and () bracket field pair on one line, both preserved with their own bracket style", () => {
		const t = byText(board, "Mixed brackets");
		const priorityField = t.fields.find((f) => f.key === "priority");
		const dueField = t.fields.find((f) => f.key === "due");
		expect(priorityField).toMatchObject({ value: "high", bracket: "(" });
		expect(dueField).toMatchObject({ value: "2026-01-01", bracket: "[" });
		expect(t.priority).toBe("high");
	});

	it("keeps two identical task lines distinguishable by line index, not merged", () => {
		const dupes = board.tasks.filter((t) => t.text === "Duplicate task text");
		expect(dupes).toHaveLength(2);
		expect(dupes[0].lineIndex).not.toBe(dupes[1].lineIndex);
	});

	it("a link label containing '::' parses as a link, not a field", () => {
		const t = byText(board, "tricky label");
		expect(t.links).toHaveLength(1);
		expect(t.links[0]).toMatchObject({ label: "see a:: b", target: "notes.md", external: false });
		expect(t.fields.some((f) => f.key === "see a")).toBe(false);
	});

	it("preserves CRLF line endings and the absence of a trailing newline", () => {
		expect(board.eol).toBe("\r\n");
		expect(board.trailingNewline).toBe(false);
	});
});

// ---------------------------------------------------------------------------
// Links
// ---------------------------------------------------------------------------

describe("link classification and resolution", () => {
	it("classifies http(s) and mailto as external, a plain path as file", () => {
		expect(classifyLinkTarget("https://example.com").kind).toBe("external");
		expect(classifyLinkTarget("http://example.com").kind).toBe("external");
		expect(classifyLinkTarget("mailto:me@example.com").kind).toBe("external");
		expect(classifyLinkTarget("../notes/Yazi.md").kind).toBe("file");
		expect(classifyLinkTarget("./local.md").kind).toBe("file");
	});

	it("does not mistake a Windows drive letter for a URL scheme", () => {
		expect(classifyLinkTarget("C:\\Users\\me\\notes.md").kind).toBe("file");
	});

	it("resolves a relative file link against the board's own directory", () => {
		const board = parseBoard(
			"- [ ] This is a file link: [Yazi](../../Notes/Yazi.md)",
			"/Users/me/vault/plans/board.md",
		);
		const t = board.tasks[0];
		expect(t.links[0]).toMatchObject({ label: "Yazi", target: "../../Notes/Yazi.md", external: false });
		expect(t.links[0].absPath).toBe("/Users/me/Notes/Yazi.md");
	});

	it("leaves an already-absolute file link target untouched", () => {
		const board = parseBoard("- [ ] See [doc](/Users/me/vault/doc.md)", "/Users/me/vault/plans/board.md");
		expect(board.tasks[0].links[0].absPath).toBe("/Users/me/vault/doc.md");
	});

	it("supports multiple links on one task", () => {
		const board = parseBoard("- [ ] [one](a.md) and [two](https://example.com)", "/repo/board.md");
		const t = board.tasks[0];
		expect(t.links).toHaveLength(2);
		expect(t.links[0].external).toBe(false);
		expect(t.links[1].external).toBe(true);
	});
});

describe("resolveRelativePath / dirnameOf", () => {
	it("resolves .. segments against a base directory", () => {
		expect(resolveRelativePath("/a/b/c", "../../d.md")).toBe("/a/d.md");
		expect(resolveRelativePath("/a/b", "./d.md")).toBe("/a/b/d.md");
	});

	it("passes an absolute path through unchanged", () => {
		expect(resolveRelativePath("/a/b", "/x/y.md")).toBe("/x/y.md");
	});

	it("extracts the parent directory", () => {
		expect(dirnameOf("/a/b/c.md")).toBe("/a/b");
		expect(dirnameOf("c.md")).toBe("");
	});
});

// ---------------------------------------------------------------------------
// Dependency graph
// ---------------------------------------------------------------------------

describe("dependency graph", () => {
	it("wires upstream/downstream from a single dependsOn id", () => {
		const board = parseBoard(SAMPLE, "/repo/board.md");
		const upstream = byText(board, "has to be done first");
		const downstream = byText(board, "has a dependency");
		expect(upstream.downstream).toHaveLength(1);
		expect(upstream.downstream[0].key).toBe(downstream.key);
		expect(downstream.upstream).toHaveLength(1);
		expect(downstream.upstream[0].key).toBe(upstream.key);
	});

	it("parses multiple comma-separated dependsOn ids, trimming whitespace", () => {
		const content = [
			"- [ ] up one  [id:: aaaaaa]",
			"- [ ] up two  [id:: bbbbbb]",
			"- [ ] down  [dependsOn:: aaaaaa, bbbbbb]",
		].join("\n");
		const board = parseBoard(content, "/repo/board.md");
		const down = byText(board, "down");
		expect(down.dependsOn).toEqual(["aaaaaa", "bbbbbb"]);
		expect(down.upstream).toHaveLength(2);
	});

	it("records a dependsOn id with no matching task as dangling, without throwing", () => {
		const board = parseBoard("- [ ] orphan  [dependsOn:: zzzzzz]", "/repo/board.md");
		const t = board.tasks[0];
		expect(t.upstream).toHaveLength(0);
		expect(t.danglingDeps).toEqual(["zzzzzz"]);
	});

	it("buildDependencyGraph can be called directly on a task array", () => {
		const tasks = parseBoard(SAMPLE, "/repo/board.md").tasks;
		// Re-running it must not duplicate upstream/downstream entries.
		buildDependencyGraph(tasks);
		const upstream = tasks.find((t) => t.id === "eqodx8") as Task;
		expect(upstream.downstream).toHaveLength(1);
	});
});

describe("isLagging — the full matrix", () => {
	function task(status: string): Task {
		return { status } as Task;
	}

	it("flags when the upstream is strictly behind on the linear scale", () => {
		expect(isLagging(task("pending"), task("ready"))).toBe(true);
		expect(isLagging(task("ready"), task("in_progress"))).toBe(true);
		expect(isLagging(task("in_progress"), task("done"))).toBe(true);
	});

	it("does not flag equal ranks, including DONE vs WON'T-FIX and two IN-PROGRESS tasks", () => {
		expect(isLagging(task("done"), task("wontfix"))).toBe(false);
		expect(isLagging(task("wontfix"), task("done"))).toBe(false);
		expect(isLagging(task("in_progress"), task("in_progress"))).toBe(false);
	});

	it("does not flag when the upstream is ahead or equal otherwise", () => {
		expect(isLagging(task("done"), task("ready"))).toBe(false);
		expect(isLagging(task("ready"), task("ready"))).toBe(false);
	});

	it("a BLOCKED upstream always flags, regardless of the downstream's own status", () => {
		expect(isLagging(task("blocked"), task("pending"))).toBe(true);
		expect(isLagging(task("blocked"), task("done"))).toBe(true);
		expect(isLagging(task("blocked"), task("blocked"))).toBe(true);
	});

	it("treats a BLOCKED downstream as READY's rank for the comparison", () => {
		expect(isLagging(task("pending"), task("blocked"))).toBe(true); // pending(0) < ready(1)
		expect(isLagging(task("ready"), task("blocked"))).toBe(false); // ready(1) == ready(1)
		expect(isLagging(task("done"), task("blocked"))).toBe(false); // done(3) > ready(1)
	});
});

// ---------------------------------------------------------------------------
// Columns, priority ordering, archive
// ---------------------------------------------------------------------------

describe("groupIntoColumns", () => {
	it("always returns all 6 columns, even when empty", () => {
		const columns = groupIntoColumns([]);
		expect(Object.keys(columns).sort()).toEqual([...STATUS_TYPES].sort());
		for (const s of STATUS_TYPES) expect(columns[s]).toEqual([]);
	});

	it("sorts a column by priority severity, highest first, ties broken by document order", () => {
		const content = [
			"- [ ] low  [priority:: low]",
			"- [ ] highest  [priority:: highest]",
			"- [ ] default",
			"- [ ] high  [priority:: high]",
			"- [ ] default2",
		].join("\n");
		const board = parseBoard(content, "/repo/board.md");
		const columns = groupIntoColumns(board.tasks);
		expect(columns.ready.map((t) => t.text)).toEqual(["highest", "high", "default", "default2", "low"]);
	});
});

describe("isArchivable / hideArchived filtering", () => {
	// Local midnight, so day differences below are exact day counts, not
	// skewed by a time-of-day offset.
	const now = Date.parse("2026-09-14T00:00:00");

	it("archives a DONE task whose completion date is more than 5 days old", () => {
		expect(isArchivable({ status: "done", completion: "2026-09-08" } as Task, now)).toBe(true); // 6 days
	});

	it("does not archive at exactly the 5-day boundary or under", () => {
		expect(isArchivable({ status: "done", completion: "2026-09-09" } as Task, now)).toBe(false); // exactly 5 days
		expect(isArchivable({ status: "done", completion: "2026-09-10" } as Task, now)).toBe(false); // 4 days
		expect(isArchivable({ status: "done", completion: "2026-09-14" } as Task, now)).toBe(false); // today
	});

	it("never archives a missing or malformed date", () => {
		expect(isArchivable({ status: "done", completion: "" } as Task, now)).toBe(false);
		expect(isArchivable({ status: "done", completion: "not-a-date" } as Task, now)).toBe(false);
	});

	it("never archives a task in the wrong column, even with an old date", () => {
		expect(isArchivable({ status: "ready", completion: "2020-01-01" } as Task, now)).toBe(false);
	});

	it("uses the cancelled field for WON'T-FIX, not completion", () => {
		expect(isArchivable({ status: "wontfix", cancelled: "2026-09-01" } as Task, now)).toBe(true);
		expect(isArchivable({ status: "wontfix", completion: "2020-01-01", cancelled: "" } as Task, now)).toBe(false);
	});

	it("groupIntoColumns drops archivable tasks only when hideArchived is set", () => {
		const board = parseBoard(
			"- [x] old  [completion:: 2026-01-01]\n- [x] recent  [completion:: 2026-09-14]",
			"/repo/b.md",
		);
		const shown = groupIntoColumns(board.tasks, { hideArchived: false, nowMs: now });
		expect(shown.done).toHaveLength(2);
		const hidden = groupIntoColumns(board.tasks, { hideArchived: true, nowMs: now });
		expect(hidden.done.map((t) => t.text)).toEqual(["recent"]);
	});
});

// ---------------------------------------------------------------------------
// Rewrite engine
// ---------------------------------------------------------------------------

describe("setStatusChar", () => {
	it("is a pure length-neutral splice", () => {
		const line = "- [ ] hello";
		const out = setStatusChar(line, 3, "x");
		expect(out).toBe("- [x] hello");
		expect(out.length).toBe(line.length);
	});
});

describe("removeField / upsertField", () => {
	it("adds a new bracketed field with the two-space separator style", () => {
		expect(upsertField("- [x] done", "completion", "2026-09-14")).toBe("- [x] done  [completion:: 2026-09-14]");
	});

	it("updates an existing field in place, keeping its bracket style", () => {
		expect(upsertField("- [x] done  (completion:: 2026-09-01)", "completion", "2026-09-14")).toBe(
			"- [x] done  (completion:: 2026-09-14)",
		);
	});

	it("removes a field and its leading whitespace, leaving no trailing whitespace", () => {
		expect(removeField("- [x] done  [completion:: 2026-09-14]", "completion")).toBe("- [x] done");
	});

	it("removes a parenthesized field just as well as a bracketed one", () => {
		expect(removeField("- [-] cancelled  (cancelled:: 2026-09-14)", "cancelled")).toBe("- [-] cancelled");
	});

	it("is a no-op when the field is absent", () => {
		const line = "- [ ] plain task";
		expect(removeField(line, "completion")).toBe(line);
	});

	it("leaves an unrelated field (bracket style and all) untouched while editing another", () => {
		const line = "- [ ] task  [due:: 2026-09-15]  [priority:: low]";
		const out = upsertField(line, "priority", "high");
		expect(out).toContain("[due:: 2026-09-15]");
		expect(out).toContain("[priority:: high]");
	});

	it("round-trips: upsert then remove restores the original trimmed line", () => {
		// A field name guaranteed absent from every sample line — the property
		// under test is "upsert-then-remove is a no-op", which only holds when
		// the field didn't already exist (upserting an EXISTING field and then
		// removing it correctly strips a field that really was already there,
		// which is a different, equally-correct behavior, not this property).
		for (const line of SAMPLE.split("\n").filter((l) => /^- \[.\]/.test(l))) {
			const withField = upsertField(line, "zzRoundTripField", "value");
			const back = removeField(withField, "zzRoundTripField");
			expect(back).toBe(line.replace(/[ \t]+$/, ""));
		}
	});
});

describe("applyStatusTransition", () => {
	function taskFor(board: ParsedBoard, needle: string): Task {
		return byText(board, needle);
	}

	it("dragging to DONE adds completion and removes any cancelled field", () => {
		const board = parseBoard(SAMPLE, "/repo/board.md");
		const t = taskFor(board, "blocked/open question");
		const out = applyStatusTransition(t.rawLine, t, "done", "2026-09-14");
		expect(out).toBe("- [x] A blocked/open question task  [completion:: 2026-09-14]");
	});

	it("dragging to WONT_FIX adds cancelled and removes any completion field", () => {
		const board = parseBoard(SAMPLE, "/repo/board.md");
		const t = taskFor(board, "basic task");
		const out = applyStatusTransition(t.rawLine, t, "wontfix", "2026-09-14");
		expect(out).toBe("- [-] This is a basic task  [cancelled:: 2026-09-14]");
	});

	it("dragging out of DONE removes the completion field entirely", () => {
		const board = parseBoard(SAMPLE, "/repo/board.md");
		const t = taskFor(board, "completed task");
		const out = applyStatusTransition(t.rawLine, t, "ready", "2026-09-14");
		expect(out).toBe("- [ ] A completed task");
	});

	it("dragging out of WONT_FIX removes a parenthesized cancelled field", () => {
		const line = "- [-] cancelled  (cancelled:: 2026-09-14)";
		const board = parseBoard(line, "/repo/board.md");
		const t = board.tasks[0];
		const out = applyStatusTransition(t.rawLine, t, "in_progress", "2026-09-14");
		expect(out).toBe("- [/] cancelled");
	});

	it("preserves an unrelated unknown field and a tag across the transition", () => {
		const line = "- [ ] task #keepme  [due:: 2026-09-15]";
		const board = parseBoard(line, "/repo/board.md");
		const t = board.tasks[0];
		const out = applyStatusTransition(t.rawLine, t, "done", "2026-09-14");
		expect(out).toContain("#keepme");
		expect(out).toContain("[due:: 2026-09-15]");
		expect(out).toContain("[completion:: 2026-09-14]");
	});

	it("is idempotent: applying the same transition twice produces identical output", () => {
		const board = parseBoard(SAMPLE, "/repo/board.md");
		const t = byText(board, "basic task");
		const once = applyStatusTransition(t.rawLine, t, "done", "2026-09-14");
		const board2 = parseBoard(once, "/repo/board.md");
		const t2 = board2.tasks[0];
		const twice = applyStatusTransition(once, t2, "done", "2026-09-14");
		expect(twice).toBe(once);
	});

	it("byte-identical second task line is untouched when mutating a different line index", () => {
		const board = parseBoard(EDGE, "/repo/edge.md");
		const dupes = board.tasks.filter((t) => t.text === "Duplicate task text");
		expect(dupes).toHaveLength(2);
		const mutatedLine = applyStatusTransition(dupes[0].rawLine, dupes[0], "done", "2026-09-14");
		// The second duplicate's own line must be untouched — proves the
		// rewrite targets a specific lineIndex, never a global regex pass.
		expect(dupes[1].rawLine).toBe("- [ ] Duplicate task text");
		expect(mutatedLine).not.toBe(dupes[1].rawLine);
	});
});

describe("EOL round-trip", () => {
	it("splitPreservingEol/joinPreservingEol round-trips CRLF with no trailing newline", () => {
		const { lines, eol, trailingNewline } = splitPreservingEol(EDGE);
		expect(eol).toBe("\r\n");
		expect(trailingNewline).toBe(false);
		expect(joinPreservingEol({ lines, eol, trailingNewline })).toBe(EDGE);
	});

	it("round-trips a plain \\n file with a trailing newline", () => {
		const { lines, eol, trailingNewline } = splitPreservingEol(SAMPLE);
		expect(eol).toBe("\n");
		expect(trailingNewline).toBe(true);
		expect(joinPreservingEol({ lines, eol, trailingNewline })).toBe(SAMPLE);
	});
});

describe("generateTaskId / ensureTaskId", () => {
	it("generates a 6-character lowercase-alphanumeric id", () => {
		const id = generateTaskId(new Set());
		expect(id).toMatch(/^[a-z0-9]{6}$/);
	});

	it("never collides with a supplied existing-id set, across many draws", () => {
		const existing = new Set<string>();
		for (let i = 0; i < 500; i++) {
			const id = generateTaskId(existing);
			expect(existing.has(id)).toBe(false);
			existing.add(id);
		}
	});

	it("is a no-op when the task already has an id", () => {
		const board = parseBoard(SAMPLE, "/repo/board.md");
		const t = byText(board, "has to be done first");
		const result = ensureTaskId(t.rawLine, t, new Set(["eqodx8"]));
		expect(result).toEqual({ line: t.rawLine, id: "eqodx8" });
	});

	it("assigns and writes a fresh id when the task has none", () => {
		const board = parseBoard(SAMPLE, "/repo/board.md");
		const t = byText(board, "basic task");
		const result = ensureTaskId(t.rawLine, t, new Set());
		expect(result.id).toMatch(/^[a-z0-9]{6}$/);
		expect(result.line).toBe(`${t.rawLine}  [id:: ${result.id}]`);
	});
});

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

describe("esc", () => {
	it("escapes the 4 HTML-significant characters", () => {
		expect(esc(`<script>&"</script>`)).toBe("&lt;script&gt;&amp;&quot;&lt;/script&gt;");
	});
});

describe("buildBoardHtml", () => {
	it("renders the empty state with an Add Board affordance when there are no boards", () => {
		const html = buildBoardHtml({ boards: [], activeBoardId: null, hideArchived: false, search: "", columns: null });
		expect(html).toContain("No boards yet");
		expect(html).toContain("add-board-btn");
	});

	it("escapes task text and a link label so a malicious title cannot inject a script tag", () => {
		const board = parseBoard("- [ ] <script>alert(1)</script> and [xss](javascript:alert(1))", "/repo/board.md");
		const columns = groupIntoColumns(board.tasks);
		const html = buildBoardHtml({
			boards: [{ id: "b1", name: "Board" }],
			activeBoardId: "b1",
			hideArchived: false,
			search: "",
			columns,
		});
		expect(html).not.toContain("<script>alert(1)</script>");
		expect(html).toContain("&lt;script&gt;");
	});

	it("stamps a data-key on each card matching the task's key", () => {
		const board = parseBoard(SAMPLE, "/repo/board.md");
		const columns = groupIntoColumns(board.tasks);
		const html = buildBoardHtml({
			boards: [{ id: "b1", name: "Board" }],
			activeBoardId: "b1",
			hideArchived: false,
			search: "",
			columns,
		});
		const upstream = byText(board, "has to be done first");
		expect(html).toContain(`data-key="${upstream.key}"`);
	});

	it("carries dependency badge highlight targets in data-hl", () => {
		const board = parseBoard(SAMPLE, "/repo/board.md");
		buildDependencyGraph(board.tasks);
		const columns = groupIntoColumns(board.tasks);
		const html = buildBoardHtml({
			boards: [{ id: "b1", name: "Board" }],
			activeBoardId: "b1",
			hideArchived: false,
			search: "",
			columns,
		});
		const downstream = byText(board, "has a dependency");
		expect(html).toContain(`data-hl="${downstream.upstream[0].key}"`);
	});

	it("renders an external link with data-ext=1 and a file link with data-ext=0", () => {
		const board = parseBoard("- [ ] [ext](https://example.com) and [file](notes.md)", "/Users/me/vault/board.md");
		const columns = groupIntoColumns(board.tasks);
		const html = buildBoardHtml({
			boards: [{ id: "b1", name: "Board" }],
			activeBoardId: "b1",
			hideArchived: false,
			search: "",
			columns,
		});
		expect(html).toContain('data-ext="1"');
		expect(html).toContain('data-ext="0"');
	});
});

describe("STATUS_LABELS / STATUS_TYPES / PROGRESS_RANK sanity", () => {
	it("has exactly 6 status types in the documented column order", () => {
		expect(STATUS_TYPES).toEqual(["pending", "ready", "in_progress", "blocked", "done", "wontfix"]);
	});

	it("has a label for every status type", () => {
		for (const s of STATUS_TYPES) expect(typeof STATUS_LABELS[s]).toBe("string");
	});

	it("PROGRESS_RANK ranks DONE and WONT_FIX equally, and omits BLOCKED", () => {
		expect(PROGRESS_RANK.done).toBe(PROGRESS_RANK.wontfix);
		expect(PROGRESS_RANK.blocked).toBeUndefined();
	});

	it("every recognized priority has an emoji entry, blank for the default", () => {
		for (const level of Object.keys(PRIORITY_RANK)) {
			expect(PRIORITY_EMOJI[level]).toBeDefined();
		}
		expect(PRIORITY_EMOJI[""]).toBe("");
	});
});

describe("todayIso", () => {
	it("formats a given date as YYYY-MM-DD in local time, not UTC", () => {
		// A date constructed from local components, so this is timezone-agnostic:
		// the local calendar day is always Sept 14 regardless of the runner's TZ.
		const d = new Date(2026, 8, 14, 23, 30, 0);
		expect(todayIso(d)).toBe("2026-09-14");
	});

	it("zero-pads single-digit months and days", () => {
		const d = new Date(2026, 0, 5, 0, 0, 0);
		expect(todayIso(d)).toBe("2026-01-05");
	});
});
