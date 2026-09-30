/**
 * Claude Usage — types and ticker formatting.
 *
 * The lifecycle (polling, ticker writes, switching between agents) lives in
 * `agentUsage.ts`, which drives both Claude and Codex through one ticker slot.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface RateBucket {
	utilization: number;
	resets_at: string | null;
}

export interface UsageApiResponse {
	five_hour: RateBucket | null;
	seven_day: RateBucket | null;
	seven_day_oauth_apps: RateBucket | null;
	seven_day_opus: RateBucket | null;
	seven_day_sonnet: RateBucket | null;
	seven_day_cowork: RateBucket | null;
	extra_usage: unknown;
	plan: unknown;
	meta: unknown;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Compact countdown to a reset, e.g. "3d", "5h", "12m".
 * Returns null when there is no date or the reset is in the past.
 * Days use the same floor as the dashboard's formatResetTime so the two agree. */
export function formatResetCompact(isoStr: string | null, now: number = Date.now()): string | null {
	if (!isoStr) return null;
	const diffMs = new Date(isoStr).getTime() - now;
	if (Number.isNaN(diffMs) || diffMs <= 0) return null;
	const diffMin = Math.floor(diffMs / 60_000);
	if (diffMin < 60) return `${diffMin}m`;
	const diffHrs = Math.floor(diffMin / 60);
	if (diffHrs < 24) return `${diffHrs}h`;
	return `${Math.floor(diffHrs / 24)}d`;
}

/** Build status bar ticker text from API data.
 * The API returns utilization as a direct percentage (e.g. 3.0 = 3%, 68.0 = 68%). */
export function buildTickerText(api: UsageApiResponse, now: number = Date.now()): string {
	const parts: string[] = [];
	if (api.five_hour) {
		parts.push(`5h: ${Math.round(api.five_hour.utilization)}%`);
	}
	if (api.seven_day) {
		const reset = formatResetCompact(api.seven_day.resets_at, now);
		const suffix = reset ? ` -${reset}` : "";
		parts.push(`7d: ${Math.round(api.seven_day.utilization)}%${suffix}`);
	}
	return parts.length > 0 ? parts.join(" · ") : "no data";
}

/** Determine ticker priority from usage levels.
 * Utilization values are direct percentages (0-100). */
export function getTickerPriority(api: UsageApiResponse): number {
	const utils = [api.five_hour, api.seven_day, api.seven_day_opus, api.seven_day_sonnet]
		.filter((b): b is RateBucket => b !== null)
		.map((b) => b.utilization);
	const maxUtil = Math.max(0, ...utils);
	if (maxUtil >= 90) return 90;
	if (maxUtil >= 70) return 50;
	return 10;
}
