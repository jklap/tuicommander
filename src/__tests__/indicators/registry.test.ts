import { describe, expect, it } from "vitest";
import { isKnownAnimationId } from "../../indicators/animations";
import { isKnownIconId } from "../../indicators/icons";
import {
	GROUP_LABELS,
	INDICATORS,
	indicatorsByGroup,
	resolveAnimationId,
	resolveIconId,
} from "../../indicators/registry";

describe("indicator registry — structural invariants", () => {
	it("has unique ids", () => {
		const ids = INDICATORS.map((entry) => entry.id);
		expect(new Set(ids).size).toBe(ids.length);
	});

	it("every id follows the <group-prefix>.<name> convention", () => {
		for (const entry of INDICATORS) {
			expect(entry.id).toMatch(/^[a-zA-Z]+\.[a-zA-Z-]+$/);
		}
	});

	it("every entry declares at least one capability", () => {
		for (const entry of INDICATORS) {
			expect(entry.capabilities.length).toBeGreaterThan(0);
		}
	});

	it("color capability implies a colorVar + defaultColor, and vice versa", () => {
		for (const entry of INDICATORS) {
			if (entry.capabilities.includes("color")) {
				expect(entry.colorVar, entry.id).toBeTruthy();
				expect(entry.defaultColor, entry.id).toBeTruthy();
			} else {
				expect(entry.colorVar, entry.id).toBeUndefined();
			}
		}
	});

	it("animation capability implies an animVar + defaultAnimation, and vice versa", () => {
		for (const entry of INDICATORS) {
			if (entry.capabilities.includes("animation")) {
				expect(entry.animVar, entry.id).toBeTruthy();
				expect(entry.defaultAnimation, entry.id).toBeTruthy();
			} else {
				expect(entry.animVar, entry.id).toBeUndefined();
			}
		}
	});

	it("icon capability implies a defaultIconId, and vice versa", () => {
		for (const entry of INDICATORS) {
			if (entry.capabilities.includes("icon")) {
				expect(entry.defaultIconId, entry.id).toBeTruthy();
			} else {
				expect(entry.defaultIconId, entry.id).toBeUndefined();
			}
		}
	});

	it("every defaultColor is a var() reference, never a raw hex — themes must keep flowing through", () => {
		for (const entry of INDICATORS) {
			if (entry.defaultColor) {
				expect(entry.defaultColor, entry.id).toMatch(/^var\(--[a-z0-9-]+\)$/);
			}
		}
	});

	it("every colorVar and animVar starts with --ind-", () => {
		for (const entry of INDICATORS) {
			if (entry.colorVar) expect(entry.colorVar, entry.id).toMatch(/^--ind-/);
			if (entry.animVar) expect(entry.animVar, entry.id).toMatch(/^--ind-anim-/);
		}
	});

	it("every defaultIconId resolves to a known icon", () => {
		for (const entry of INDICATORS) {
			if (entry.defaultIconId) expect(isKnownIconId(entry.defaultIconId), entry.id).toBe(true);
		}
	});

	it("every defaultAnimation resolves to a known animation", () => {
		for (const entry of INDICATORS) {
			if (entry.defaultAnimation) expect(isKnownAnimationId(entry.defaultAnimation), entry.id).toBe(true);
		}
	});

	it("every declared `animations` list contains its own defaultAnimation", () => {
		for (const entry of INDICATORS) {
			if (entry.animations && entry.defaultAnimation) {
				expect(entry.animations, entry.id).toContain(entry.defaultAnimation);
			}
		}
	});

	it("every group referenced by an entry has a label", () => {
		for (const entry of INDICATORS) {
			expect(GROUP_LABELS[entry.group], entry.group).toBeTruthy();
		}
	});

	it("indicatorsByGroup returns only entries of that group, and covers every entry exactly once", () => {
		const groups = Object.keys(GROUP_LABELS) as (keyof typeof GROUP_LABELS)[];
		let total = 0;
		for (const group of groups) {
			const entries = indicatorsByGroup(group);
			total += entries.length;
			for (const entry of entries) expect(entry.group).toBe(group);
		}
		expect(total).toBe(INDICATORS.length);
	});
});

describe("resolveIconId / resolveAnimationId — the single source of truth every icon-rendering site must go through", () => {
	it("resolveIconId returns the entry's default when no override is set", () => {
		expect(resolveIconId([], "sidebar.main")).toBe("star");
	});

	it("resolveIconId returns the override when one is set and valid", () => {
		expect(resolveIconId([{ id: "sidebar.main", icon: "diamond" }], "sidebar.main")).toBe("diamond");
	});

	it("resolveIconId ignores an override for a different id", () => {
		expect(resolveIconId([{ id: "sidebar.worktree", icon: "diamond" }], "sidebar.main")).toBe("star");
	});

	it("resolveIconId falls back to the default when the override value is not a known icon", () => {
		// sanitizeIndicatorOverrides should already have dropped this before it reaches the
		// store, but resolveIconId is a second line of defense at the render chokepoint.
		expect(resolveIconId([{ id: "sidebar.main", icon: "not-a-real-icon" }], "sidebar.main")).toBe("star");
	});

	it("resolveIconId falls back to 'dot' for an id with no registry entry and no default", () => {
		expect(resolveIconId([], "not.a.real.indicator")).toBe("dot");
	});

	it("resolveAnimationId returns the entry's default when no override is set", () => {
		expect(resolveAnimationId([], "terminal.busy")).toBe("pulse");
	});

	it("resolveAnimationId returns the override when one is set and valid", () => {
		expect(resolveAnimationId([{ id: "terminal.busy", animation: "blink" }], "terminal.busy")).toBe("blink");
	});

	it("resolveAnimationId falls back to the default when the override value is not a known animation", () => {
		expect(resolveAnimationId([{ id: "terminal.busy", animation: "not-a-real-animation" }], "terminal.busy")).toBe(
			"pulse",
		);
	});
});
