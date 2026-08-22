import { describe, expect, it } from "vitest";
import { pickBlock } from "../../utils/blockNav";

describe("pickBlock", () => {
	const promptLines = [0, 10, 25, 50];

	it("picks the closest prompt line strictly above viewTop for 'previous'", () => {
		expect(pickBlock(promptLines, 30, "previous")).toBe(25);
	});

	it("picks the closest prompt line strictly below viewTop for 'next'", () => {
		expect(pickBlock(promptLines, 30, "next")).toBe(50);
	});

	it("returns undefined when already at the first block ('previous')", () => {
		expect(pickBlock(promptLines, 0, "previous")).toBeUndefined();
	});

	it("returns undefined when already at the last block ('next')", () => {
		expect(pickBlock(promptLines, 50, "next")).toBeUndefined();
	});

	it("treats an exact match on viewTop as not-above/not-below (strict comparison)", () => {
		expect(pickBlock(promptLines, 25, "previous")).toBe(10);
		expect(pickBlock(promptLines, 25, "next")).toBe(50);
	});

	it("returns undefined for an empty list", () => {
		expect(pickBlock([], 15, "previous")).toBeUndefined();
		expect(pickBlock([], 15, "next")).toBeUndefined();
	});
});
