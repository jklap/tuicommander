import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { applyIndicatorOverrides } from "../../indicators/apply";
import { INDICATORS } from "../../indicators/registry";

/** All --ind-* / --ind-anim-* vars this suite touches, so afterEach can
 *  restore document.documentElement.style to a clean slate between tests. */
function allIndicatorVars(): string[] {
	const vars: string[] = [];
	for (const entry of INDICATORS) {
		if (entry.colorVar) vars.push(entry.colorVar);
		if (entry.animVar) vars.push(entry.animVar);
	}
	return vars;
}

describe("applyIndicatorOverrides", () => {
	beforeEach(() => {
		for (const v of allIndicatorVars()) document.documentElement.style.removeProperty(v);
	});

	afterEach(() => {
		for (const v of allIndicatorVars()) document.documentElement.style.removeProperty(v);
	});

	it("sets the color var for an overridden indicator", () => {
		applyIndicatorOverrides([{ id: "terminal.busy", color: "#ff00ff" }]);
		expect(document.documentElement.style.getPropertyValue("--ind-terminal-busy")).toBe("#ff00ff");
	});

	it("sets the animation var (resolved to the full keyframe shorthand, not the bare id)", () => {
		applyIndicatorOverrides([{ id: "terminal.busy", animation: "blink" }]);
		const value = document.documentElement.style.getPropertyValue("--ind-anim-terminal-busy");
		expect(value).toContain("tuic-blink");
	});

	it("removes the color var when an override is cleared (full-list re-apply, not a delta)", () => {
		applyIndicatorOverrides([{ id: "terminal.busy", color: "#ff00ff" }]);
		expect(document.documentElement.style.getPropertyValue("--ind-terminal-busy")).toBe("#ff00ff");

		applyIndicatorOverrides([]);
		expect(document.documentElement.style.getPropertyValue("--ind-terminal-busy")).toBe("");
	});

	it("leaves other indicators' vars untouched", () => {
		applyIndicatorOverrides([{ id: "terminal.busy", color: "#ff00ff" }]);
		expect(document.documentElement.style.getPropertyValue("--ind-terminal-idle")).toBe("");
	});

	it("rejects an unsafe color and removes any stale value instead of injecting it", () => {
		applyIndicatorOverrides([{ id: "terminal.busy", color: "#ff00ff" }]);
		applyIndicatorOverrides([{ id: "terminal.busy", color: "javascript:alert(1)" }]);
		expect(document.documentElement.style.getPropertyValue("--ind-terminal-busy")).toBe("");
	});

	it("rejects an unknown animation id", () => {
		applyIndicatorOverrides([{ id: "terminal.busy", animation: "not-a-real-animation" }]);
		expect(document.documentElement.style.getPropertyValue("--ind-anim-terminal-busy")).toBe("");
	});

	it("ignores an override for an id the registry doesn't know about, without throwing", () => {
		expect(() => applyIndicatorOverrides([{ id: "not.a.real.indicator", color: "#ff00ff" }])).not.toThrow();
	});

	it("accepts a var() reference so a theme keeps flowing through", () => {
		applyIndicatorOverrides([{ id: "terminal.busy", color: "var(--accent)" }]);
		expect(document.documentElement.style.getPropertyValue("--ind-terminal-busy")).toBe("var(--accent)");
	});

	it("handles multiple overrides across different groups in one call", () => {
		applyIndicatorOverrides([
			{ id: "terminal.busy", color: "#ff00ff" },
			{ id: "pr.conflict", color: "#00ff00", animation: "pulse-slow" },
		]);
		expect(document.documentElement.style.getPropertyValue("--ind-terminal-busy")).toBe("#ff00ff");
		expect(document.documentElement.style.getPropertyValue("--ind-pr-conflict")).toBe("#00ff00");
		expect(document.documentElement.style.getPropertyValue("--ind-anim-pr-conflict")).toContain("tuic-pulse");
	});

	// tabType's colorVar is a raw "r, g, b" triple (consumed inside rgba()), unlike every
	// other group's plain color var — ColorPickerDialog only ever hands back #rrggbb hex, so
	// this conversion is required for a tabType override to render at all instead of silently
	// producing an invalid `rgba(#ff00ff, 0.1)` the browser drops.
	describe("tabType -rgb vars", () => {
		it("converts a 6-digit hex override into a comma-separated r, g, b triple", () => {
			applyIndicatorOverrides([{ id: "tabType.diff", color: "#ff00ff" }]);
			expect(document.documentElement.style.getPropertyValue("--ind-tabtype-diff-rgb")).toBe("255, 0, 255");
		});

		it("converts a shorthand 3-digit hex override", () => {
			applyIndicatorOverrides([{ id: "tabType.editor", color: "#0f0" }]);
			expect(document.documentElement.style.getPropertyValue("--ind-tabtype-editor-rgb")).toBe("0, 255, 0");
		});

		it("converts an rgb()/rgba() override by extracting its components", () => {
			applyIndicatorOverrides([{ id: "tabType.markdown", color: "rgba(10, 20, 30, 0.5)" }]);
			expect(document.documentElement.style.getPropertyValue("--ind-tabtype-markdown-rgb")).toBe("10, 20, 30");
		});

		it("removes the var rather than writing an unconvertable var() reference verbatim", () => {
			applyIndicatorOverrides([{ id: "tabType.panel", color: "var(--accent)" }]);
			expect(document.documentElement.style.getPropertyValue("--ind-tabtype-panel-rgb")).toBe("");
		});

		it("does not touch a plain (non -rgb) color var's format", () => {
			applyIndicatorOverrides([{ id: "terminal.busy", color: "#ff00ff" }]);
			expect(document.documentElement.style.getPropertyValue("--ind-terminal-busy")).toBe("#ff00ff");
		});
	});
});
