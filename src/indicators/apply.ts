import { INDICATOR_ANIMATIONS, isKnownAnimationId } from "./animations";
import { INDICATORS, type IndicatorOverride } from "./registry";
import { isSafeIndicatorColor } from "./validate";

const HEX_RE = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
const RGB_FN_RE = /^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*(?:,\s*[\d.]{1,5}\s*)?\)$/;

/**
 * Every `--ind-*-rgb` var (tabType's colorVar convention — see `IndicatorDef.defaultColor`'s
 * doc comment) is a raw "r, g, b" triple consumed inside `rgba(var(...), alpha)` by the
 * consuming CSS, never a standalone color. `ColorPickerDialog` only ever produces `#rrggbb`
 * hex — writing that verbatim into a `-rgb` var makes every `rgba(var(--ind-tabtype-diff-rgb),
 * 0.1)` an invalid computed value, so the browser drops the whole declaration and the tab's
 * background/border silently vanish instead of showing the picked color.
 */
function toRgbTriple(color: string): string | null {
	const hex = HEX_RE.exec(color);
	if (hex) {
		let h = hex[1];
		if (h.length === 3 || h.length === 4) {
			h = [...h.slice(0, 3)].map((c) => c + c).join("");
		} else {
			h = h.slice(0, 6);
		}
		const r = Number.parseInt(h.slice(0, 2), 16);
		const g = Number.parseInt(h.slice(2, 4), 16);
		const b = Number.parseInt(h.slice(4, 6), 16);
		return `${r}, ${g}, ${b}`;
	}
	const rgbFn = RGB_FN_RE.exec(color);
	if (rgbFn) return `${rgbFn[1]}, ${rgbFn[2]}, ${rgbFn[3]}`;
	// A `var(--token)` override can't be resolved to a literal triple here — fall back to
	// removing the override below rather than writing something the CSS can't consume.
	return null;
}

/**
 * Writes every indicator override's resolved color/animation onto
 * `document.documentElement.style`, and `removeProperty`s any `--ind-*` var
 * that no longer has an override — so the `global.css` default resurfaces
 * instead of a stale inline value lingering after a reset.
 *
 * Takes the FULL current override list (not a delta) and walks the whole
 * registry, because "no override for this indicator" and "override just
 * removed" look identical from here — both need `removeProperty`, and only
 * a full walk can tell the two apart from a partial list.
 *
 * Called from the tail of `themes.ts`'s `applyAppTheme()` (which already
 * covers every window: main, detached panels, FloatingTerminal) and from a
 * `createEffect` in `useAppearanceSync.ts` for the overrides-changed-without-
 * a-theme-change case.
 *
 * Revalidates color/animation values even though `settingsStore`'s setters
 * and hydrate path already do (indicators/validate.ts) — this is the actual
 * DOM-write chokepoint, so it gets its own defense in depth rather than
 * trusting every caller upstream got it right.
 */
export function applyIndicatorOverrides(overrides: readonly IndicatorOverride[]): void {
	const root = document.documentElement.style;
	const byId = new Map(overrides.map((o) => [o.id, o]));

	for (const entry of INDICATORS) {
		if (entry.colorVar) {
			const color = byId.get(entry.id)?.color;
			const isRgbTriple = entry.colorVar.endsWith("-rgb");
			const resolved = color && isSafeIndicatorColor(color) ? (isRgbTriple ? toRgbTriple(color) : color) : null;
			if (resolved) {
				root.setProperty(entry.colorVar, resolved);
			} else {
				root.removeProperty(entry.colorVar);
			}
		}
		if (entry.animVar) {
			const animation = byId.get(entry.id)?.animation;
			if (animation && isKnownAnimationId(animation)) {
				root.setProperty(entry.animVar, INDICATOR_ANIMATIONS[animation]);
			} else {
				root.removeProperty(entry.animVar);
			}
		}
	}
}
