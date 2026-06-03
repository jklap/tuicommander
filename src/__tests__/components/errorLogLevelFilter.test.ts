import { describe, expect, it } from "vitest";
import { levelPassesThreshold } from "../../components/ErrorLogPanel/ErrorLogPanel";
import type { AppLogLevel } from "../../stores/appLogger";

const ALL_LEVELS: AppLogLevel[] = ["debug", "info", "warn", "error"];

describe("levelPassesThreshold", () => {
	it("shows every level when the filter is 'all'", () => {
		for (const level of ALL_LEVELS) {
			expect(levelPassesThreshold(level, "all")).toBe(true);
		}
	});

	it("shows the selected level and everything more severe", () => {
		// "warn" intermingles warn + error, hides debug + info
		expect(levelPassesThreshold("error", "warn")).toBe(true);
		expect(levelPassesThreshold("warn", "warn")).toBe(true);
		expect(levelPassesThreshold("info", "warn")).toBe(false);
		expect(levelPassesThreshold("debug", "warn")).toBe(false);
	});

	it("shows only errors at the highest threshold", () => {
		expect(levelPassesThreshold("error", "error")).toBe(true);
		expect(levelPassesThreshold("warn", "error")).toBe(false);
		expect(levelPassesThreshold("info", "error")).toBe(false);
		expect(levelPassesThreshold("debug", "error")).toBe(false);
	});

	it("shows all levels at the lowest threshold (debug)", () => {
		for (const level of ALL_LEVELS) {
			expect(levelPassesThreshold(level, "debug")).toBe(true);
		}
	});
});
