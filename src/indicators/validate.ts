import { isKnownAnimationId } from "./animations";
import { isKnownIconId } from "./icons";
import { getIndicator, type IndicatorOverride } from "./registry";

/**
 * Colors are written straight onto `document.documentElement.style` (see
 * `apply.ts`) as the VALUE of a CSS custom property. A hand-edited
 * `config.json` is untrusted input reaching that sink, so every override is
 * revalidated here on hydrate as well as on write — accepting only the
 * handful of forms an indicator color could legitimately take:
 *   - `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`
 *   - `rgb(n, n, n)` / `rgba(n, n, n, a)`
 *   - `var(--kebab-case-name)` (chaining to another token, incl. themes)
 * Anything else (arbitrary `url()`, `expression()`, unbalanced parens, etc.)
 * is rejected outright rather than sanitized — there is no legitimate
 * indicator color shape this would ever reject.
 */
const HEX_COLOR_RE = /^#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
const RGB_COLOR_RE = /^rgba?\(\s*\d{1,3}\s*,\s*\d{1,3}\s*,\s*\d{1,3}\s*(?:,\s*[\d.]{1,5}\s*)?\)$/;
const VAR_COLOR_RE = /^var\(\s*--[a-z0-9-]+\s*\)$/;

export function isSafeIndicatorColor(value: string): boolean {
	return HEX_COLOR_RE.test(value) || RGB_COLOR_RE.test(value) || VAR_COLOR_RE.test(value);
}

export function isSafeIndicatorIconId(value: string): boolean {
	return isKnownIconId(value);
}

export function isSafeIndicatorAnimationId(value: string): boolean {
	return isKnownAnimationId(value);
}

/**
 * Drops anything a hand-edited config.json could use to inject CSS, and
 * anything referencing an id/icon/animation this build's registry doesn't
 * know about (e.g. a stale override for a removed indicator). Called on
 * hydrate — never trust a value from disk into `document.documentElement.style`
 * unchecked.
 */
export function sanitizeIndicatorOverrides(overrides: readonly IndicatorOverride[]): IndicatorOverride[] {
	const result: IndicatorOverride[] = [];
	for (const override of overrides) {
		if (!getIndicator(override.id)) continue;
		const clean: IndicatorOverride = { id: override.id };
		if (override.color !== undefined && isSafeIndicatorColor(override.color)) clean.color = override.color;
		if (override.icon !== undefined && isSafeIndicatorIconId(override.icon)) clean.icon = override.icon;
		if (override.animation !== undefined && isSafeIndicatorAnimationId(override.animation)) {
			clean.animation = override.animation;
		}
		// An override with every field stripped carries no information — drop it
		// rather than persist a bare { id } that does nothing.
		if (clean.color !== undefined || clean.icon !== undefined || clean.animation !== undefined) {
			result.push(clean);
		}
	}
	return result;
}
