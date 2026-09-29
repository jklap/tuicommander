export interface TruncatePatchResult {
	patch: string;
	hiddenLines: number;
}

interface ParsedHunk {
	/** Original "@@ -a,b +c,d @@ suffix" line, used verbatim when the whole hunk survives truncation. */
	headerLine: string;
	oldStart: number;
	newStart: number;
	/** Text after the closing "@@" on the hunk header (often a function name) — preserved on a rewritten header. */
	headerSuffix: string;
	/** One entry per content line (' '/'+'/'-' prefixed, or blank). A following
	 *  "\ No newline at end of file" marker line is merged into the preceding
	 *  entry so it always travels with whichever line survives truncation. */
	lines: string[];
}

interface ParsedSection {
	/** "diff --git"/"index"/"---"/"+++" lines preceding this file's first hunk — always kept, never counted. */
	headerLines: string[];
	hunks: ParsedHunk[];
}

const HUNK_HEADER_RE = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)$/;
const NO_NEWLINE_MARKER = "\\ No newline at end of file";

function parsePatch(patch: string): ParsedSection[] {
	const rawLines = patch.split("\n");
	const sections: ParsedSection[] = [];
	let current: ParsedSection | null = null;
	let currentHunk: ParsedHunk | null = null;

	const ensureSection = (): ParsedSection => {
		if (!current) {
			current = { headerLines: [], hunks: [] };
			sections.push(current);
		}
		return current;
	};

	for (const line of rawLines) {
		if (line.startsWith("diff --git ")) {
			current = { headerLines: [line], hunks: [] };
			sections.push(current);
			currentHunk = null;
			continue;
		}
		const hunkMatch = line.match(HUNK_HEADER_RE);
		if (hunkMatch) {
			const section = ensureSection();
			currentHunk = {
				headerLine: line,
				oldStart: Number(hunkMatch[1]),
				newStart: Number(hunkMatch[2]),
				headerSuffix: hunkMatch[3] ?? "",
				lines: [],
			};
			section.hunks.push(currentHunk);
			continue;
		}
		if (line.startsWith(NO_NEWLINE_MARKER)) {
			if (currentHunk && currentHunk.lines.length > 0) {
				currentHunk.lines[currentHunk.lines.length - 1] += `\n${line}`;
			} else {
				ensureSection().headerLines.push(line);
			}
			continue;
		}
		if (currentHunk) {
			currentHunk.lines.push(line);
		} else {
			ensureSection().headerLines.push(line);
		}
	}
	return sections;
}

/**
 * Truncate a unified diff patch to at most `maxLines` content lines, cutting
 * at hunk boundaries — a hunk that's split mid-way gets its header's
 * old/new line counts rewritten to match what survived, so the emitted
 * patch is always internally consistent. File header lines ("diff --git",
 * "index", "---", "+++") are never counted against the budget and are
 * always kept.
 *
 * `maxLines <= 0` or a patch that already fits means "never truncate" — the
 * input is returned unchanged (not reformatted) with `hiddenLines: 0`.
 */
export function truncatePatch(patch: string, maxLines: number): TruncatePatchResult {
	if (maxLines <= 0) return { patch, hiddenLines: 0 };

	const sections = parsePatch(patch);
	const totalContentLines = sections.reduce(
		(sum, section) => sum + section.hunks.reduce((s, h) => s + h.lines.length, 0),
		0,
	);
	if (totalContentLines <= maxLines) return { patch, hiddenLines: 0 };

	let budget = maxLines;
	let hiddenLines = 0;
	const outParts: string[] = [];

	for (const section of sections) {
		outParts.push(...section.headerLines);
		for (const hunk of section.hunks) {
			if (budget <= 0) {
				hiddenLines += hunk.lines.length;
				continue;
			}
			if (hunk.lines.length <= budget) {
				outParts.push(hunk.headerLine, ...hunk.lines);
				budget -= hunk.lines.length;
				continue;
			}
			const keptLines = hunk.lines.slice(0, budget);
			hiddenLines += hunk.lines.length - budget;
			let oldCount = 0;
			let newCount = 0;
			for (const l of keptLines) {
				if (l.startsWith("-")) oldCount++;
				else if (l.startsWith("+")) newCount++;
				else {
					oldCount++;
					newCount++;
				}
			}
			const newHeader = `@@ -${hunk.oldStart},${oldCount} +${hunk.newStart},${newCount} @@${hunk.headerSuffix}`;
			outParts.push(newHeader, ...keptLines);
			budget = 0;
		}
	}

	return { patch: outParts.join("\n"), hiddenLines };
}
