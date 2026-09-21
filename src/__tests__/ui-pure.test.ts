import { describe, expect, it } from "vitest";
import { validateBranchName } from "../components/RenameBranchDialog/RenameBranchDialog";
import { trimSelection } from "../components/Terminal/Terminal";
import { parseDiff, parseDiffFiles } from "../components/ui/DiffViewer";
import { stripAnsi } from "../utils/stripAnsi";

describe("stripAnsi", () => {
	it("strips color codes", () => {
		expect(stripAnsi("\x1B[31mred\x1B[0m")).toBe("red");
	});

	it("strips bold codes", () => {
		expect(stripAnsi("\x1B[1mbold\x1B[22m")).toBe("bold");
	});

	it("leaves plain text unchanged", () => {
		expect(stripAnsi("hello world")).toBe("hello world");
	});

	it("handles empty string", () => {
		expect(stripAnsi("")).toBe("");
	});

	it("strips multiple ANSI codes", () => {
		expect(stripAnsi("\x1B[32m\x1B[1mgreen bold\x1B[0m")).toBe("green bold");
	});

	it("strips codes with multiple parameters", () => {
		expect(stripAnsi("\x1B[38;5;196mred256\x1B[0m")).toBe("red256");
	});
});

describe("parseDiff", () => {
	it("identifies header lines", () => {
		const lines = parseDiff("diff --git a/file.ts b/file.ts");
		expect(lines[0].type).toBe("header");
		expect(lines[0].content).toBe("diff --git a/file.ts b/file.ts");
	});

	it("identifies hunk lines", () => {
		const lines = parseDiff("@@ -1,5 +1,6 @@");
		expect(lines[0].type).toBe("hunk");
	});

	it("identifies addition lines", () => {
		const lines = parseDiff("+new line");
		expect(lines[0].type).toBe("addition");
	});

	it("does not treat +++ as addition", () => {
		const lines = parseDiff("+++ b/file.ts");
		expect(lines[0].type).toBe("context");
	});

	it("identifies deletion lines", () => {
		const lines = parseDiff("-old line");
		expect(lines[0].type).toBe("deletion");
	});

	it("does not treat --- as deletion", () => {
		const lines = parseDiff("--- a/file.ts");
		expect(lines[0].type).toBe("context");
	});

	it("identifies context lines", () => {
		const lines = parseDiff(" unchanged line");
		expect(lines[0].type).toBe("context");
	});

	it("parses a full diff correctly", () => {
		const diff = [
			"diff --git a/file.ts b/file.ts",
			"--- a/file.ts",
			"+++ b/file.ts",
			"@@ -1,3 +1,4 @@",
			" unchanged",
			"-removed",
			"+added",
			"+also added",
		].join("\n");

		const lines = parseDiff(diff);
		expect(lines[0].type).toBe("header");
		expect(lines[1].type).toBe("context"); // ---
		expect(lines[2].type).toBe("context"); // +++
		expect(lines[3].type).toBe("hunk");
		expect(lines[4].type).toBe("context");
		expect(lines[5].type).toBe("deletion");
		expect(lines[6].type).toBe("addition");
		expect(lines[7].type).toBe("addition");
	});

	it("handles empty diff", () => {
		const lines = parseDiff("");
		expect(lines).toHaveLength(1);
		expect(lines[0].content).toBe("");
	});
});

describe("parseDiffFiles", () => {
	it("splits a multi-file diff into file sections", () => {
		const diff = [
			"diff --git a/file1.ts b/file1.ts",
			"--- a/file1.ts",
			"+++ b/file1.ts",
			"@@ -1,3 +1,4 @@",
			" unchanged",
			"-removed",
			"+added",
			"+also added",
			"diff --git a/file2.ts b/file2.ts",
			"--- a/file2.ts",
			"+++ b/file2.ts",
			"@@ -1,2 +1,2 @@",
			"-old",
			"+new",
		].join("\n");

		const files = parseDiffFiles(diff);
		expect(files).toHaveLength(2);
		expect(files[0].path).toBe("file1.ts");
		expect(files[1].path).toBe("file2.ts");
	});

	it("counts additions and deletions per file", () => {
		const diff = [
			"diff --git a/file.ts b/file.ts",
			"--- a/file.ts",
			"+++ b/file.ts",
			"@@ -1,3 +1,4 @@",
			" unchanged",
			"-removed",
			"+added",
			"+also added",
		].join("\n");

		const files = parseDiffFiles(diff);
		expect(files[0].additions).toBe(2);
		expect(files[0].deletions).toBe(1);
	});

	it("extracts file path from diff --git header", () => {
		const diff = "diff --git a/src/components/App.tsx b/src/components/App.tsx\n@@ -1 +1 @@\n-old\n+new";
		const files = parseDiffFiles(diff);
		expect(files[0].path).toBe("src/components/App.tsx");
	});

	it("handles rename detection", () => {
		const diff = [
			"diff --git a/old.ts b/new.ts",
			"similarity index 90%",
			"rename from old.ts",
			"rename to new.ts",
			"--- a/old.ts",
			"+++ b/new.ts",
			"@@ -1,2 +1,2 @@",
			"-old",
			"+new",
		].join("\n");

		const files = parseDiffFiles(diff);
		expect(files[0].path).toBe("new.ts");
	});

	it("returns empty array for empty diff", () => {
		expect(parseDiffFiles("")).toHaveLength(0);
		expect(parseDiffFiles("  \n  ")).toHaveLength(0);
	});

	it("preserves raw lines for each file section", () => {
		const diff = ["diff --git a/a.ts b/a.ts", "--- a/a.ts", "+++ b/a.ts", "@@ -1 +1 @@", "-old", "+new"].join("\n");

		const files = parseDiffFiles(diff);
		expect(files[0].lines).toHaveLength(6);
	});

	it("handles new file mode", () => {
		const diff = [
			"diff --git a/new.ts b/new.ts",
			"new file mode 100644",
			"--- /dev/null",
			"+++ b/new.ts",
			"@@ -0,0 +1,3 @@",
			"+line1",
			"+line2",
			"+line3",
		].join("\n");

		const files = parseDiffFiles(diff);
		expect(files[0].path).toBe("new.ts");
		expect(files[0].additions).toBe(3);
		expect(files[0].deletions).toBe(0);
	});
});

describe("validateBranchName", () => {
	it("returns null for valid names", () => {
		expect(validateBranchName("main")).toBeNull();
		expect(validateBranchName("feature/test")).toBeNull();
		expect(validateBranchName("wip/my-branch")).toBeNull();
	});

	it("rejects empty names", () => {
		expect(validateBranchName("")).toBeTruthy();
		expect(validateBranchName("  ")).toBeTruthy();
	});

	it("rejects names with spaces", () => {
		expect(validateBranchName("branch name")).toContain("spaces");
	});

	it("rejects names starting with hyphen", () => {
		expect(validateBranchName("-branch")).toContain("hyphen");
	});

	it("rejects names with double dots", () => {
		expect(validateBranchName("branch..name")).toContain("..");
	});

	it("rejects names ending with .lock", () => {
		expect(validateBranchName("branch.lock")).toContain(".lock");
	});

	it("rejects names with invalid characters", () => {
		expect(validateBranchName("branch~1")).toContain("invalid");
		expect(validateBranchName("branch^1")).toContain("invalid");
		expect(validateBranchName("branch:name")).toContain("invalid");
		expect(validateBranchName("branch?")).toContain("invalid");
		expect(validateBranchName("branch*")).toContain("invalid");
		expect(validateBranchName("branch[1]")).toContain("invalid");
		expect(validateBranchName("branch\\name")).toContain("invalid");
	});

	it("rejects invalid slash usage", () => {
		expect(validateBranchName("/branch")).toContain("slash");
		expect(validateBranchName("branch/")).toContain("slash");
		expect(validateBranchName("branch//name")).toContain("slash");
	});

	it("rejects names ending with a period", () => {
		expect(validateBranchName("branch.")).toContain("period");
	});

	it("rejects names with @{", () => {
		expect(validateBranchName("branch@{1}")).toContain("@{");
	});
});

describe("trimSelection", () => {
	it("trims trailing whitespace from each line", () => {
		expect(trimSelection("hello   \nworld   ")).toBe("hello\nworld");
	});

	it("preserves leading whitespace", () => {
		expect(trimSelection("  indented   \n    more   ")).toBe("  indented\n    more");
	});

	it("handles empty lines", () => {
		expect(trimSelection("line1   \n   \nline3   ")).toBe("line1\n\nline3");
	});

	it("handles single line", () => {
		expect(trimSelection("single line   ")).toBe("single line");
	});

	it("handles empty string", () => {
		expect(trimSelection("")).toBe("");
	});

	it("handles lines with no trailing whitespace", () => {
		expect(trimSelection("clean\nalready")).toBe("clean\nalready");
	});
});
