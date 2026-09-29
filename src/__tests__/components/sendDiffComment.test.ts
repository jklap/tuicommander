import { describe, expect, it, vi } from "vitest";
import { formatDiffComment, sendDiffComment } from "../../components/DiffTab/sendDiffComment";

describe("formatDiffComment", () => {
	it("formats a single selected line as [path:Lx]", () => {
		const msg = formatDiffComment(
			{
				filePath: "src/foo.ts",
				startLine: 10,
				endLine: 10,
				lines: [{ lineNumber: 10, type: "+", content: "const x = 1;" }],
			},
			"looks off",
		);
		expect(msg).toBe("[src/foo.ts:L10]\n+const x = 1;\n— looks off");
	});

	it("formats a multi-line range as [path:Lx-Ly]", () => {
		const msg = formatDiffComment(
			{
				filePath: "src/foo.ts",
				startLine: 10,
				endLine: 12,
				lines: [
					{ lineNumber: 10, type: "+", content: "a" },
					{ lineNumber: 11, type: "-", content: "b" },
					{ lineNumber: 12, type: " ", content: "c" },
				],
			},
			"note",
		);
		expect(msg).toBe("[src/foo.ts:L10-L12]\n+a\n-b\n c\n— note");
	});

	it("preserves +/-/space line prefixes", () => {
		const msg = formatDiffComment(
			{
				filePath: "f.ts",
				startLine: 1,
				endLine: 2,
				lines: [
					{ lineNumber: 1, type: "+", content: "added" },
					{ lineNumber: 2, type: "-", content: "removed" },
				],
			},
			"x",
		);
		expect(msg).toContain("+added");
		expect(msg).toContain("-removed");
	});

	it("strips CR/NUL from code line content", () => {
		const msg = formatDiffComment(
			{ filePath: "f.ts", startLine: 1, endLine: 1, lines: [{ lineNumber: 1, type: "+", content: "a\r\0b" }] },
			"x",
		);
		expect(msg).toBe("[f.ts:L1]\n+ab\n— x");
	});

	it("strips CR/LF/NUL from the file path", () => {
		const msg = formatDiffComment(
			{ filePath: "f\r\n\0.ts", startLine: 1, endLine: 1, lines: [{ lineNumber: 1, type: " ", content: "x" }] },
			"x",
		);
		expect(msg.startsWith("[f.ts:L1]")).toBe(true);
	});
});

describe("sendDiffComment", () => {
	const target = {
		filePath: "f.ts",
		startLine: 1,
		endLine: 1,
		lines: [{ lineNumber: 1, type: "+" as const, content: "x" }],
	};

	it("returns 'empty' for blank text without calling send", async () => {
		const send = vi.fn();
		const result = await sendDiffComment(target, "   ", "sess-1", "claude", send);
		expect(result).toBe("empty");
		expect(send).not.toHaveBeenCalled();
	});

	it("returns 'no-terminal' when no session id is given", async () => {
		const send = vi.fn();
		const result = await sendDiffComment(target, "note", undefined, "claude", send);
		expect(result).toBe("no-terminal");
		expect(send).not.toHaveBeenCalled();
	});

	it("returns 'empty' when there are no selected lines", async () => {
		const send = vi.fn();
		const result = await sendDiffComment({ ...target, lines: [] }, "note", "sess-1", "claude", send);
		expect(result).toBe("empty");
		expect(send).not.toHaveBeenCalled();
	});

	it("calls send with the formatted message and returns 'ok'", async () => {
		const send = vi.fn().mockResolvedValue(undefined);
		const result = await sendDiffComment(target, "note", "sess-1", "claude", send);
		expect(result).toBe("ok");
		expect(send).toHaveBeenCalledWith("sess-1", "[f.ts:L1]\n+x\n— note", "claude");
	});

	it("passes null agentType through as null, not undefined", async () => {
		const send = vi.fn().mockResolvedValue(undefined);
		await sendDiffComment(target, "note", "sess-1", null, send);
		expect(send).toHaveBeenCalledWith("sess-1", expect.any(String), null);
	});

	it("returns 'error' when send throws", async () => {
		const send = vi.fn().mockRejectedValue(new Error("boom"));
		const result = await sendDiffComment(target, "note", "sess-1", "claude", send);
		expect(result).toBe("error");
	});
});
