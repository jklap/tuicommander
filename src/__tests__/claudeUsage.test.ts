import { describe, expect, it } from "vitest";
import "./mocks/tauri";
import { buildTickerText, formatResetCompact } from "../features/claudeUsage";

const NOW = Date.UTC(2026, 5, 11, 12, 0, 0); // fixed reference instant

function isoIn(ms: number): string {
	return new Date(NOW + ms).toISOString();
}

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

describe("formatResetCompact", () => {
	it("returns days when more than a day remains", () => {
		expect(formatResetCompact(isoIn(3 * DAY + 5 * HOUR), NOW)).toBe("3d");
	});

	it("returns hours when less than a day remains", () => {
		expect(formatResetCompact(isoIn(5 * HOUR + 30 * 60_000), NOW)).toBe("5h");
	});

	it("returns minutes when less than an hour remains", () => {
		expect(formatResetCompact(isoIn(12 * 60_000), NOW)).toBe("12m");
	});

	it("returns null for a past or null reset", () => {
		expect(formatResetCompact(isoIn(-HOUR), NOW)).toBeNull();
		expect(formatResetCompact(null, NOW)).toBeNull();
		expect(formatResetCompact("not-a-date", NOW)).toBeNull();
	});
});

describe("buildTickerText", () => {
	const empty = {
		five_hour: null,
		seven_day: null,
		seven_day_oauth_apps: null,
		seven_day_opus: null,
		seven_day_sonnet: null,
		seven_day_cowork: null,
		extra_usage: null,
		plan: null,
		meta: null,
	};

	it("appends the compact countdown to the 7d bucket", () => {
		const text = buildTickerText({
			...empty,
			five_hour: { utilization: 12, resets_at: null },
			seven_day: { utilization: 5, resets_at: isoIn(3 * DAY + 5 * HOUR) },
		});
		expect(text).toBe("5h: 12% · 7d: 5% -3d");
	});

	it("omits the countdown when the 7d bucket has no reset date", () => {
		const text = buildTickerText({
			...empty,
			seven_day: { utilization: 68, resets_at: null },
		});
		expect(text).toBe("7d: 68%");
	});

	it("returns 'no data' when no buckets are present", () => {
		expect(buildTickerText(empty)).toBe("no data");
	});
});
