import type { AutomationAdapter, AutomationDefinition } from "../components/AutomationsDialog/contract";
export const definition: AutomationDefinition = {
	id: "a1",
	name: "Sentry",
	prompt: "Review new issues",
	repository: "/repo",
	run_config: "claude:unattended",
	workspace: { mode: "existing" },
	cron: "*/30 * * * *",
	timezone: "Europe/Madrid",
	enabled: true,
	grace_secs: 43200,
	overlap: "skip",
	max_duration_secs: 1500,
	precheck: null,
};
export function fakeAdapter(overrides: Partial<AutomationAdapter> = {}): AutomationAdapter {
	return {
		list: async () => [{ definition, next_run_ms: 1791568800000, last_status: "skipped_overlap" }],
		save: async (value) => value,
		remove: async () => {},
		runNow: async () => ({ status: "skipped_concurrency", reason: "All slots are busy" }),
		history: async () => [],
		preview: async (cron, timezone) => ({
			cron,
			timezone: timezone || "Europe/Madrid",
			occurrences: ["2026-10-09T19:00:00Z"],
		}),
		preset: async () => ({ cron: "30 7 * * 1-5" }),
		...overrides,
	};
}
