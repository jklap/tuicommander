/**
 * Named animation choices offered on customizable indicators, mapped to the
 * full CSS `animation` shorthand. Values reference the six global `tuic-*`
 * keyframes defined in `global.css` — kept OUT of CSS Modules deliberately,
 * since `@keyframes` names are scoped per module and this table's values
 * must resolve to the same keyframe from every consuming CSS file.
 *
 * "pulse" and "pulse-slow" are today's real `pulse-opacity` phase
 * (0.4 → 1 → 0.4) at the two durations already in use (1.5s for the busy
 * dot / PR conflict badge, 2s for the CI-pending/checking badges) — every
 * indicator that currently pulses must default to one of these two exact
 * ids so wiring the registry in produces no visible change.
 */
export type AnimationId = "none" | "pulse" | "pulse-slow" | "blink" | "breathe" | "glow" | "spin";

export const INDICATOR_ANIMATIONS: Record<AnimationId, string> = {
	none: "none",
	pulse: "tuic-pulse 1.5s ease-in-out infinite",
	"pulse-slow": "tuic-pulse 2s ease-in-out infinite",
	blink: "tuic-blink 1s steps(1, end) infinite",
	breathe: "tuic-breathe 3s ease-in-out infinite",
	glow: "tuic-glow 1.8s ease-in-out infinite",
	spin: "tuic-spin 1s linear infinite",
};

/** Human-readable labels for the animation picker (Phase 3). */
export const ANIMATION_LABELS: Record<AnimationId, string> = {
	none: "None",
	pulse: "Pulse",
	"pulse-slow": "Pulse (slow)",
	blink: "Blink",
	breathe: "Breathe (dim → solid → dim)",
	glow: "Glow",
	spin: "Spin",
};

export function isKnownAnimationId(id: string): id is AnimationId {
	// Not `id in INDICATOR_ANIMATIONS` — see the identical comment in icons.ts's
	// isKnownIconId for why `in` is unsafe here, and why this doesn't use Object.hasOwn.
	// biome-ignore lint: Object.hasOwn isn't in the ES2021 lib target this project uses.
	return Object.prototype.hasOwnProperty.call(INDICATOR_ANIMATIONS, id);
}
