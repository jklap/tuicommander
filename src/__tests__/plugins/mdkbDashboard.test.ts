import { describe, expect, it } from "vitest";
// @ts-expect-error — untyped plugin JS module in the plugins submodule.
import { renderDashboard } from "../../../plugins/mdkb-dashboard/main.js";

function dashboard(hooks: Record<string, unknown>) {
	const page = new DOMParser().parseFromString(
		renderDashboard({ stats: { hooks }, memories: [], config: null, error: null }, "/repo"),
		"text/html",
	);
	return page.body;
}

// Domain inputs follow mdkb HooksSummary in src/cli/stats_report.rs:236.
// This is a renderer contract, not a recorded external CLI fixture.
const event = { event: "session_start", invocations: 12, fired: 8, avg_ms: 3, p95_ms: 7 };

describe("mdkb dashboard hook telemetry", () => {
	// Catches: disabled telemetry with no log rows silently hides the Hooks section.
	it("disabled hook telemetry is explained instead of hidden as zero activity", () => {
		const body = dashboard({ telemetry_enabled: false, events: [] });
		expect(body.textContent).toContain("Hook telemetry is off");
		expect(body.textContent).not.toContain("Calls");
	});

	// Catches: retained log rows are presented as current activity after telemetry is disabled.
	it("disabled hook telemetry suppresses retained activity metrics", () => {
		const body = dashboard({ telemetry_enabled: false, events: [event], slow_events_7d: 4 });
		expect(body.textContent).toContain("Hook telemetry is off");
		expect(body.querySelector("table")).toBeNull();
		expect(body.textContent).not.toContain("slow (7d)");
	});

	// Catches: enabled telemetry with no recorded events silently hides the Hooks section.
	it("enabled hook telemetry with no events shows zero activity", () => {
		const body = dashboard({ telemetry_enabled: true, events: [], slow_events_7d: 0 });
		expect([...body.querySelectorAll("h2")].map((heading) => heading.textContent)).toContain("Hooks");
		expect(body.textContent).toContain("No hook activity recorded");
		expect(body.textContent).not.toContain("Hook telemetry is off");
	});

	// Catches: a truthiness check treats older MDKB reports as disabled telemetry.
	it.each([true, undefined])("enabled or legacy telemetry (%s) retains event metrics", (enabled) => {
		const body = dashboard({ ...(enabled === undefined ? {} : { telemetry_enabled: enabled }), events: [event] });
		expect(body.textContent).not.toContain("Hook telemetry is off");
		expect(body.querySelector("tbody")?.textContent).toContain("session_start");
		expect([...body.querySelectorAll("tbody td")].map((cell) => cell.textContent)).toEqual([
			"session_start",
			"12",
			"8",
			"3",
			"7",
		]);
	});
});
