import { describe, expect, it } from "vitest";
import { filePathRegex } from "../../../components/Terminal/linkProvider";

function candidates(row: string): string[] {
	const re = filePathRegex();
	const out: string[] = [];
	let m: RegExpExecArray | null;
	while ((m = re.exec(row)) !== null) out.push(m[1]);
	return out;
}

const PATH = "e2e/fixtures/video-downloadpp.mp4";

// Each candidate is sent to the backend's existence check verbatim, so a path with
// a stray character glued to it is never underlined — the failure is silent.
describe("a candidate is the path itself, not the prose or markup around it", () => {
	// Catches: the sentence's own period captured as part of the path (`x.mp4.`).
	it.each([
		[`saved to ${PATH}.`, PATH],
		[`Wrote ${PATH}.`, PATH],
		[`(see ${PATH}).`, PATH],
		[`/abs/dir/${"clip.mp4"}.`, "/abs/dir/clip.mp4"],
		[`~/src/${"a.txt"}.`, "~/src/a.txt"],
		[`C:\\Users\\me\\clip.mp4.`, "C:\\Users\\me\\clip.mp4"],
	])("row %j yields exactly %j", (row, want) => {
		expect(candidates(row)).toEqual([want]);
	});

	// Catches: markdown bold (`**path**`) hiding the path entirely.
	it.each([
		[`**${PATH}**`, PATH],
		[`Created **${PATH}** (12 MB)`, PATH],
		[`- **${PATH}**: the fixture`, PATH],
	])("row %j yields exactly %j", (row, want) => {
		expect(candidates(row)).toEqual([want]);
	});

	// Catches: an ellipsis-truncated path losing its extension or vanishing.
	it("ends a match at an ellipsis", () => {
		expect(candidates(`file: ${PATH}…`)).toEqual([PATH]);
	});

	// Catches: the trailing-dot rule eating the line:col suffix or the ordinary cases.
	it.each([
		[`error at src/a.ts:12:3 here`, "src/a.ts:12:3"],
		[`  ${PATH}`, PATH],
		[`\t       ${PATH}`, PATH],
		[`\`${PATH}\``, PATH],
		[`${PATH}, ${PATH}`, PATH],
		[`./a/b.sh ../c/d.sh`, "./a/b.sh"],
	])("still matches %j", (row, want) => {
		expect(candidates(row)).toContain(want);
	});

	it("keeps a dot inside the path and in a hidden directory", () => {
		expect(candidates("see src/a.b/c.d.ts.")).toEqual(["src/a.b/c.d.ts"]);
		expect(candidates("see .github/workflows/ci.yml.")).toEqual([".github/workflows/ci.yml"]);
	});

	it("does not turn a glob into a path", () => {
		expect(candidates("run src/**/*.ts now")).not.toContain("src/**/*.ts");
	});
});
