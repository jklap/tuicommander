import { describe, expect, it } from "vitest";
import { truncatePatch } from "../../utils/truncatePatch";

const HEADER = "diff --git a/f.ts b/f.ts\nindex abc..def 100644\n--- a/f.ts\n+++ b/f.ts";

function hunk(oldStart: number, newStart: number, lines: string[], suffix = ""): string {
	const oldCount = lines.filter((l) => !l.startsWith("+")).length;
	const newCount = lines.filter((l) => !l.startsWith("-")).length;
	return [`@@ -${oldStart},${oldCount} +${newStart},${newCount} @@${suffix}`, ...lines].join("\n");
}

describe("truncatePatch", () => {
	it("maxLines <= 0 never truncates", () => {
		const patch = `${HEADER}\n${hunk(1, 1, [" a", "-b", "+c", " d"])}`;
		expect(truncatePatch(patch, 0)).toEqual({ patch, hiddenLines: 0 });
		expect(truncatePatch(patch, -5)).toEqual({ patch, hiddenLines: 0 });
	});

	it("a patch smaller than maxLines is returned unchanged", () => {
		const patch = `${HEADER}\n${hunk(1, 1, [" a", "-b", "+c"])}`;
		const result = truncatePatch(patch, 100);
		expect(result).toEqual({ patch, hiddenLines: 0 });
	});

	it("exact-boundary: a patch with exactly maxLines content lines is unchanged", () => {
		const lines = [" a", "-b", "+c", " d"];
		const patch = `${HEADER}\n${hunk(1, 1, lines)}`;
		const result = truncatePatch(patch, lines.length);
		expect(result).toEqual({ patch, hiddenLines: 0 });
	});

	it("truncation landing exactly on a hunk boundary needs no header rewrite", () => {
		const h1Lines = [" a", "-b", "+c"];
		const h2Lines = [" x", "+y"];
		const patch = `${HEADER}\n${hunk(1, 1, h1Lines)}\n${hunk(10, 10, h2Lines)}`;
		const result = truncatePatch(patch, h1Lines.length);

		expect(result.hiddenLines).toBe(h2Lines.length);
		// The first hunk's header is untouched (verbatim), and the second hunk
		// (including its own header) is fully absent from the output.
		expect(result.patch).toBe(`${HEADER}\n${hunk(1, 1, h1Lines)}`);
		expect(result.patch).not.toContain("@@ -10");
	});

	it("truncation landing mid-hunk rewrites that hunk's header counts", () => {
		const lines = [" a", "-b", "+c", " d", "+e", "-f"];
		const patch = `${HEADER}\n${hunk(5, 5, lines, " someFn()")}`;
		// Keep only the first 3 lines: " a", "-b", "+c" -> old: a,b (2), new: a,c (2)
		const result = truncatePatch(patch, 3);

		expect(result.hiddenLines).toBe(3);
		expect(result.patch).toBe(`${HEADER}\n@@ -5,2 +5,2 @@ someFn()\n a\n-b\n+c`);
	});

	it("cuts off part of the LAST included hunk and still emits its header with corrected counts", () => {
		const h1Lines = [" a", " b"]; // 2 lines, fully included
		const h2Lines = ["-x", "+y", " z", "+w"]; // budget only allows 2 of these 4
		const patch = `${HEADER}\n${hunk(1, 1, h1Lines)}\n${hunk(20, 20, h2Lines)}`;
		const result = truncatePatch(patch, 4);

		expect(result.hiddenLines).toBe(2);
		// h1 kept verbatim (2 lines), h2 truncated to its first 2 lines: "-x","+y" -> old:1 (x), new:1 (y)
		expect(result.patch).toBe(`${HEADER}\n${hunk(1, 1, h1Lines)}\n@@ -20,1 +20,1 @@\n-x\n+y`);
	});

	it("preserves a trailing '\\ No newline at end of file' marker on whichever line survives", () => {
		const patch = `${HEADER}\n@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file`;
		// Only the first line fits — the marker was attached to the now-dropped "-b" line, so it must not leak into the output.
		const result = truncatePatch(patch, 1);
		expect(result.patch).not.toContain("No newline");
		expect(result.hiddenLines).toBe(1);

		// Both lines fit — the marker travels with "-b" since it's merged into that entry.
		const full = truncatePatch(patch, 2);
		expect(full).toEqual({ patch, hiddenLines: 0 });
	});

	it("a header-only file section (e.g. a pure rename with no hunks) is always kept, uncounted", () => {
		const renameOnly = "diff --git a/old.ts b/new.ts\nsimilarity index 100%\nrename from old.ts\nrename to new.ts";
		const real = `${HEADER}\n${hunk(1, 1, [" a", "-b", "+c", " d"])}`;
		const patch = `${renameOnly}\n${real}`;
		const result = truncatePatch(patch, 1);

		expect(result.patch.startsWith(renameOnly)).toBe(true);
		expect(result.hiddenLines).toBe(3);
	});
});
