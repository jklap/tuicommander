import { describe, expect, it } from "vitest";
import { buildRows } from "../../components/SessionDiffTab/buildRows";
import type { EditStep, FileReview, SessionReview } from "../../types/sessionDiff";

function step(overrides: Partial<EditStep>): EditStep {
	return {
		step_index: 0,
		tool_use_id: "toolu_1",
		timestamp: "2026-09-14T21:00:00.000Z",
		kind: "edit",
		abs_path: "/repo/a.ts",
		rel_path: "a.ts",
		in_repo: true,
		patch: "@@ -1,1 +1,1 @@\n-a\n+b\n",
		additions: 1,
		deletions: 1,
		is_sidechain: false,
		agent_name: null,
		user_modified: false,
		replace_all: false,
		...overrides,
	};
}

function fileGroup(overrides: Partial<FileReview>): FileReview {
	return {
		abs_path: "/repo/a.ts",
		rel_path: "a.ts",
		in_repo: true,
		display_path: "a.ts",
		net_change: "modified",
		base_source: "backup",
		cumulative_patch: "@@ -1,1 +1,1 @@\n-a\n+b\n",
		additions: 1,
		deletions: 1,
		step_indices: [0],
		drifted_from_disk: false,
		backup_available: true,
		is_binary: false,
		...overrides,
	};
}

function review(overrides: Partial<SessionReview>): SessionReview {
	return {
		session_id: "s1",
		transcript_path: "/x.jsonl",
		repo_path: "/repo",
		started_at: null,
		ended_at: null,
		title: null,
		steps: [],
		files: [],
		warnings: [],
		included_subagents: false,
		...overrides,
	};
}

describe("buildRows", () => {
	it("grouped mode: one row per FileGroup, steps resolved by step_index not array position", () => {
		const stepA = step({ step_index: 5, tool_use_id: "toolu_a", abs_path: "/repo/a.ts" });
		const stepB = step({ step_index: 2, tool_use_id: "toolu_b", abs_path: "/repo/a.ts" });
		const r = review({
			steps: [stepA, stepB],
			files: [fileGroup({ abs_path: "/repo/a.ts", step_indices: [5, 2] })],
		});

		const rows = buildRows(r, "file", new Set(), new Set());
		expect(rows).toHaveLength(1);
		expect(rows[0].kind).toBe("file");
		if (rows[0].kind !== "file") throw new Error("unreachable");
		// Resolved by step_index identity, in the order step_indices listed them,
		// NOT by array position in `steps`.
		expect(rows[0].steps.map((s) => s.tool_use_id)).toEqual(["toolu_a", "toolu_b"]);
	});

	it("grouped mode: a dangling step_indices entry is dropped, not crashed", () => {
		const r = review({
			steps: [step({ step_index: 0, tool_use_id: "toolu_a" })],
			files: [fileGroup({ step_indices: [0, 99] })],
		});
		const rows = buildRows(r, "file", new Set(), new Set());
		expect(rows[0].kind).toBe("file");
		if (rows[0].kind !== "file") throw new Error("unreachable");
		expect(rows[0].steps).toHaveLength(1);
		expect(rows[0].steps[0].tool_use_id).toBe("toolu_a");
	});

	it("grouped mode: expanded/stepsOpen reflect the given sets, keyed by abs_path", () => {
		const r = review({
			steps: [step({ step_index: 0 })],
			files: [
				fileGroup({ abs_path: "/repo/a.ts", step_indices: [0] }),
				fileGroup({ abs_path: "/repo/b.ts", step_indices: [] }),
			],
		});
		const rows = buildRows(r, "file", new Set(["/repo/a.ts"]), new Set(["/repo/b.ts"]));
		const a = rows.find((row) => row.kind === "file" && row.group.abs_path === "/repo/a.ts");
		const b = rows.find((row) => row.kind === "file" && row.group.abs_path === "/repo/b.ts");
		expect(a?.kind === "file" && a.expanded).toBe(true);
		expect(a?.kind === "file" && a.stepsOpen).toBe(false);
		expect(b?.kind === "file" && b.expanded).toBe(false);
		expect(b?.kind === "file" && b.stepsOpen).toBe(true);
	});

	it("chronological mode: every step, flat, sorted by step_index ascending", () => {
		const r = review({
			steps: [
				step({ step_index: 3, tool_use_id: "toolu_3" }),
				step({ step_index: 1, tool_use_id: "toolu_1" }),
				step({ step_index: 2, tool_use_id: "toolu_2" }),
			],
			files: [fileGroup({})],
		});
		const rows = buildRows(r, "chronological", new Set(), new Set());
		expect(rows.every((row) => row.kind === "step")).toBe(true);
		expect(rows.map((row) => (row.kind === "step" ? row.step.tool_use_id : null))).toEqual([
			"toolu_1",
			"toolu_2",
			"toolu_3",
		]);
	});

	it("returns an empty array for a session with no files/steps", () => {
		expect(buildRows(review({}), "file", new Set(), new Set())).toEqual([]);
		expect(buildRows(review({}), "chronological", new Set(), new Set())).toEqual([]);
	});
});
