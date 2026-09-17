import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
	ANIMATION_LABELS,
	type AnimationId,
	INDICATOR_ANIMATIONS,
	isKnownAnimationId,
} from "../../indicators/animations";

const GLOBAL_CSS = readFileSync(join(process.cwd(), "src/global.css"), "utf8");

/** Every `@keyframes <name>` actually declared in global.css. A source-text
 *  check, matching this suite's genre (transport.test.ts, registryParity.test.ts)
 *  — deliberately NOT importing anything from global.css, since CSS isn't a
 *  JS module; this is the only way to catch a keyframe rename. */
function declaredKeyframeNames(): Set<string> {
	const matches = GLOBAL_CSS.match(/@keyframes\s+([a-z0-9-]+)/g) ?? [];
	return new Set(matches.map((m) => m.replace(/@keyframes\s+/, "")));
}

describe("indicator animations", () => {
	it("every animation id (except 'none') resolves to a shorthand naming a real global.css keyframe", () => {
		const declared = declaredKeyframeNames();
		for (const [id, shorthand] of Object.entries(INDICATOR_ANIMATIONS) as [AnimationId, string][]) {
			if (id === "none") {
				expect(shorthand).toBe("none");
				continue;
			}
			const keyframeName = shorthand.split(" ")[0];
			expect(declared.has(keyframeName), `${id} → "${shorthand}" names an undeclared keyframe`).toBe(true);
		}
	});

	it("every declared tuic-* keyframe is used by at least one animation id (no dead keyframes)", () => {
		const used = new Set(Object.values(INDICATOR_ANIMATIONS).map((s) => s.split(" ")[0]));
		for (const name of declaredKeyframeNames()) {
			if (!name.startsWith("tuic-")) continue;
			expect(used.has(name), name).toBe(true);
		}
	});

	it("isKnownAnimationId recognizes every id and rejects an unknown one", () => {
		for (const id of Object.keys(INDICATOR_ANIMATIONS)) {
			expect(isKnownAnimationId(id)).toBe(true);
		}
		expect(isKnownAnimationId("not-a-real-animation")).toBe(false);
	});

	// See the identical test in icons.test.ts — INDICATOR_ANIMATIONS is also a plain object
	// literal, so `in` would wrongly approve these inherited Object.prototype keys.
	it("isKnownAnimationId rejects inherited Object.prototype keys", () => {
		for (const bogus of ["constructor", "toString", "hasOwnProperty", "__proto__", "valueOf"]) {
			expect(isKnownAnimationId(bogus), bogus).toBe(false);
		}
	});

	it("every animation id has a human-readable label", () => {
		for (const id of Object.keys(INDICATOR_ANIMATIONS) as AnimationId[]) {
			expect(ANIMATION_LABELS[id], id).toBeTruthy();
		}
	});

	it("pulse and pulse-slow use today's real durations (1.5s / 2s) so wiring changes nothing by default", () => {
		expect(INDICATOR_ANIMATIONS.pulse).toContain("1.5s");
		expect(INDICATOR_ANIMATIONS["pulse-slow"]).toContain("2s");
	});
});
