import { describe, expect, it } from "vitest";
import { ICON_IDS, type IconLayer, INDICATOR_ICON_DEFS, isKnownIconId } from "../../indicators/icons";
import { INDICATORS } from "../../indicators/registry";

/** Every layer of a def — a single layer for most icons, both layers for a
 *  composite one (clock, pause). */
function layersOf(id: (typeof ICON_IDS)[number]): readonly IconLayer[] {
	const def = INDICATOR_ICON_DEFS[id];
	return def.kind === "composite" ? def.layers : [def];
}

describe("indicator icons", () => {
	it("has at least the 18 curated icons this refactor shipped with", () => {
		expect(ICON_IDS.length).toBeGreaterThanOrEqual(18);
	});

	it("every icon has at least one non-empty drawable layer", () => {
		for (const id of ICON_IDS) {
			const layers = layersOf(id);
			expect(layers.length, id).toBeGreaterThan(0);
			for (const layer of layers) {
				if (layer.kind === "fill" || layer.kind === "stroke") {
					expect(layer.d.length, `${id} (${layer.kind})`).toBeGreaterThan(0);
				}
				if (layer.kind === "circle" || layer.kind === "ring" || layer.kind === "arc") {
					expect(layer.r, `${id} (${layer.kind})`).toBeGreaterThan(0);
				}
				if (layer.kind === "rect") {
					expect(layer.width, id).toBeGreaterThan(0);
					expect(layer.height, id).toBeGreaterThan(0);
				}
			}
		}
	});

	it("isKnownIconId recognizes every curated id and rejects an unknown one", () => {
		for (const id of ICON_IDS) expect(isKnownIconId(id)).toBe(true);
		expect(isKnownIconId("not-a-real-icon")).toBe(false);
	});

	// INDICATOR_ICON_DEFS is a plain object literal, so it inherits Object.prototype —
	// `id in INDICATOR_ICON_DEFS` would wrongly return true for these, letting a hand-edited
	// config.json entry like {"icon": "constructor"} pass validation.
	it("isKnownIconId rejects inherited Object.prototype keys", () => {
		for (const bogus of ["constructor", "toString", "hasOwnProperty", "__proto__", "valueOf"]) {
			expect(isKnownIconId(bogus), bogus).toBe(false);
		}
	});

	it("every registry entry's defaultIconId resolves to a known icon", () => {
		for (const entry of INDICATORS) {
			if (entry.defaultIconId) {
				expect(isKnownIconId(entry.defaultIconId), entry.id).toBe(true);
			}
		}
	});

	it("ICON_IDS has no duplicates", () => {
		expect(new Set(ICON_IDS).size).toBe(ICON_IDS.length);
	});
});
