import { render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";

// DiffView itself needs a real Canvas for text measurement (see the
// "DiffViewer component" describe block's own note below) — mocked here,
// keeping every other real export (DiffModeEnum, DiffFile via
// @git-diff-view/core), so these tests exercise DiffViewer's OWN
// `toModeEnum` mapping against a non-empty diff without needing Canvas.
const diffViewCalls = vi.hoisted(() => [] as unknown[]);
const diffViewWrapCalls = vi.hoisted(() => [] as unknown[]);
vi.mock("@git-diff-view/solid", async (importOriginal) => {
	const { createEffect } = await import("solid-js");
	const actual = await importOriginal<typeof import("@git-diff-view/solid")>();
	return {
		...actual,
		DiffView: (props: { diffViewMode: number; diffViewWrap?: boolean }) => {
			// A Solid component function body runs once at mount — reading
			// `props.diffViewMode` there would only ever capture the FIRST
			// value. Wrap the read in an effect so it re-runs on every mode
			// change, the same way the real DiffView's JSX binding would.
			createEffect(() => {
				diffViewCalls.push(props.diffViewMode);
				diffViewWrapCalls.push(props.diffViewWrap);
			});
			return null;
		},
	};
});

import { classifyLine, DiffViewer, parseDiff, parseDiffFiles } from "../../components/ui/DiffViewer";

describe("parseDiff", () => {
	it("classifies addition lines", () => {
		const lines = parseDiff("+added line");
		expect(lines[0].type).toBe("addition");
	});

	it("classifies deletion lines", () => {
		const lines = parseDiff("-removed line");
		expect(lines[0].type).toBe("deletion");
	});

	it("classifies header lines", () => {
		const lines = parseDiff("diff --git a/foo b/foo");
		expect(lines[0].type).toBe("header");
	});

	it("classifies hunk lines", () => {
		const lines = parseDiff("@@ -1,3 +1,4 @@");
		expect(lines[0].type).toBe("hunk");
	});

	it("classifies context lines", () => {
		const lines = parseDiff("unchanged context line");
		expect(lines[0].type).toBe("context");
	});

	it("does not classify +++ as addition", () => {
		const lines = parseDiff("+++ b/file.ts");
		expect(lines[0].type).toBe("context");
	});

	it("does not classify --- as deletion", () => {
		const lines = parseDiff("--- a/file.ts");
		expect(lines[0].type).toBe("context");
	});
});

describe("classifyLine", () => {
	it("returns header for diff --git lines", () => {
		expect(classifyLine("diff --git a/foo b/foo")).toBe("header");
	});

	it("returns hunk for @@ lines", () => {
		expect(classifyLine("@@ -1,3 +1,4 @@")).toBe("hunk");
	});

	it("returns addition for + lines", () => {
		expect(classifyLine("+new")).toBe("addition");
	});

	it("returns deletion for - lines", () => {
		expect(classifyLine("-old")).toBe("deletion");
	});

	it("returns context for +++ header", () => {
		expect(classifyLine("+++ b/file.ts")).toBe("context");
	});

	it("returns context for --- header", () => {
		expect(classifyLine("--- a/file.ts")).toBe("context");
	});
});

describe("parseDiffFiles", () => {
	it("splits multi-file diff", () => {
		const diff = [
			"diff --git a/a.ts b/a.ts",
			"@@ -1 +1 @@",
			"-old",
			"+new",
			"diff --git a/b.ts b/b.ts",
			"@@ -1 +1 @@",
			"-x",
			"+y",
		].join("\n");
		const files = parseDiffFiles(diff);
		expect(files).toHaveLength(2);
		expect(files[0].path).toBe("a.ts");
		expect(files[1].path).toBe("b.ts");
	});

	it("counts additions and deletions", () => {
		const diff = "diff --git a/f b/f\n@@ -1 +1,2 @@\n-old\n+new\n+extra";
		const files = parseDiffFiles(diff);
		expect(files[0].additions).toBe(2);
		expect(files[0].deletions).toBe(1);
	});

	it("returns empty for blank diff", () => {
		expect(parseDiffFiles("")).toHaveLength(0);
		expect(parseDiffFiles("  ")).toHaveLength(0);
	});
});

describe("DiffViewer component", () => {
	// Note: @git-diff-view/solid requires Canvas for text measurement,
	// which is not available in jsdom/happy-dom. We test empty states
	// (which don't trigger the library rendering) and verify the
	// component mounts without the library path.

	it("shows empty message when diff is empty", () => {
		const { container } = render(() => <DiffViewer diff="" />);
		const empty = container.querySelector(".diff-empty");
		expect(empty).not.toBeNull();
		expect(empty!.textContent).toBe("No changes");
	});

	it("shows custom empty message", () => {
		const { container } = render(() => <DiffViewer diff="  " emptyMessage="Nothing to show" />);
		const empty = container.querySelector(".diff-empty");
		expect(empty!.textContent).toBe("Nothing to show");
	});

	it("renders container with id", () => {
		const { container } = render(() => <DiffViewer diff="" />);
		const el = container.querySelector("#diff-content");
		expect(el).not.toBeNull();
	});

	it("shows a fallback message instead of throwing on an unparseable diff", () => {
		// Combined "@@@" merge-conflict diffs are rejected by @git-diff-view's
		// parser with "Invalid hunk header format" — must not crash the app.
		const combined = [
			"diff --cc file.ts",
			"index abc..def 100644",
			"--- a/file.ts",
			"+++ b/file.ts",
			"@@@ -1,2 -1,2 +1,3 @@@",
			"  context",
			"++added",
		].join("\n");
		const { container } = render(() => <DiffViewer diff={combined} />);
		const empty = container.querySelector(".diff-empty");
		expect(empty).not.toBeNull();
		expect(empty!.textContent).toBe("Unable to render this diff");
	});
});

describe("DiffViewer mode mapping", () => {
	const DIFF = "diff --git a/f b/f\n@@ -1 +1 @@\n-old\n+new";

	it("maps mode='split' to the library's Split enum", async () => {
		diffViewCalls.length = 0;
		render(() => <DiffViewer diff={DIFF} mode="split" />);
		await Promise.resolve();
		expect(diffViewCalls.length).toBeGreaterThan(0);
		const { DiffModeEnum } = await import("@git-diff-view/solid");
		expect(diffViewCalls.at(-1)).toBe(DiffModeEnum.Split);
	});

	it("maps mode='unified' to the library's Unified enum", async () => {
		diffViewCalls.length = 0;
		render(() => <DiffViewer diff={DIFF} mode="unified" />);
		await Promise.resolve();
		const { DiffModeEnum } = await import("@git-diff-view/solid");
		expect(diffViewCalls.at(-1)).toBe(DiffModeEnum.Unified);
	});

	it("an omitted mode falls back to Split, not Unified — `toModeEnum` only special-cases the string 'unified'", async () => {
		diffViewCalls.length = 0;
		render(() => <DiffViewer diff={DIFF} />);
		await Promise.resolve();
		const { DiffModeEnum } = await import("@git-diff-view/solid");
		expect(diffViewCalls.at(-1)).toBe(DiffModeEnum.Split);
	});

	it("re-renders with the new mode when the mode prop changes (split <-> unified switching)", async () => {
		const { DiffModeEnum } = await import("@git-diff-view/solid");
		const [mode, setMode] = createSignal<"split" | "unified">("split");
		diffViewCalls.length = 0;
		render(() => <DiffViewer diff={DIFF} mode={mode()} />);
		await Promise.resolve();
		expect(diffViewCalls.at(-1)).toBe(DiffModeEnum.Split);

		setMode("unified");
		await Promise.resolve();
		expect(diffViewCalls.at(-1)).toBe(DiffModeEnum.Unified);
	});
});

describe("DiffViewer wrap prop", () => {
	const DIFF = "diff --git a/f b/f\n@@ -1 +1 @@\n-old\n+new";

	it("defaults diffViewWrap to false when wrap is omitted", async () => {
		diffViewWrapCalls.length = 0;
		render(() => <DiffViewer diff={DIFF} />);
		await Promise.resolve();
		expect(diffViewWrapCalls.at(-1)).toBe(false);
	});

	it("forwards wrap={true} to the library's diffViewWrap prop, in both modes", async () => {
		diffViewWrapCalls.length = 0;
		render(() => <DiffViewer diff={DIFF} mode="split" wrap={true} />);
		await Promise.resolve();
		expect(diffViewWrapCalls.at(-1)).toBe(true);

		diffViewWrapCalls.length = 0;
		render(() => <DiffViewer diff={DIFF} mode="unified" wrap={true} />);
		await Promise.resolve();
		expect(diffViewWrapCalls.at(-1)).toBe(true);
	});
});

describe("DiffViewer maxLines truncation", () => {
	const bigDiff = [
		"diff --git a/f b/f",
		"index abc..def 100644",
		"--- a/f",
		"+++ b/f",
		"@@ -1,5 +1,5 @@",
		...Array.from({ length: 20 }, (_, i) => ` line ${i}`),
	].join("\n");

	it("does not truncate when maxLines is unset or 0", () => {
		const { container: unset } = render(() => <DiffViewer diff={bigDiff} />);
		expect(unset.querySelector(".diff-truncated-notice")).toBeNull();

		const { container: zero } = render(() => <DiffViewer diff={bigDiff} maxLines={0} />);
		expect(zero.querySelector(".diff-truncated-notice")).toBeNull();
	});

	it("shows a truncation notice with the hidden-line count above the budget", () => {
		const { container } = render(() => <DiffViewer diff={bigDiff} maxLines={5} />);
		const notice = container.querySelector(".diff-truncated-notice");
		expect(notice).not.toBeNull();
		expect(notice!.textContent).toContain("15");
	});

	it("clicking the notice reveals the full diff and removes the notice", async () => {
		const { container, getByText } = render(() => <DiffViewer diff={bigDiff} maxLines={5} />);
		expect(container.querySelector(".diff-truncated-notice")).not.toBeNull();

		getByText(/Show all 15 more lines/).click();
		await Promise.resolve();

		expect(container.querySelector(".diff-truncated-notice")).toBeNull();
	});

	it("a diff at or under maxLines never shows the notice", () => {
		const small = "diff --git a/f b/f\n@@ -1,1 +1,1 @@\n-a\n+b";
		const { container } = render(() => <DiffViewer diff={small} maxLines={1000} />);
		expect(container.querySelector(".diff-truncated-notice")).toBeNull();
	});
});
