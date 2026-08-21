import type { DecodedRow } from "./canvasTerminalUtils";

/** Floor of the gesture acceleration ramp — the damping applied to the first screenful of travel. */
export const GESTURE_ACCEL_MIN = 0.5;
/**
 * Ceiling of the gesture acceleration ramp.
 *
 * The ramp reaches 1.0 (exact 1:1 pixel tracking, the direct-manipulation baseline)
 * after 2 screens of cumulative travel, and this 2.0 ceiling after 4. Past 1:1 the
 * content no longer tracks the gesture at all, and macOS has already applied its own
 * momentum curve underneath us, so 2.0 is a hard ceiling on top of an already
 * accelerated input. Left uncapped, a long momentum fling could ramp past 5x, which is
 * part of what made scrolling back to a specific line unreliable.
 */
export const GESTURE_ACCEL_MAX = 2;

/** Progressive acceleration factor for a gesture that has traveled `distancePx` cumulative pixels. */
export function gestureAccelFactor(distancePx: number, screenPx: number): number {
	if (screenPx <= 0) return GESTURE_ACCEL_MIN;
	const excess = Math.max(0, distancePx - screenPx);
	return Math.min(GESTURE_ACCEL_MAX, GESTURE_ACCEL_MIN + 0.5 * (excess / screenPx));
}

export interface CanvasScrollController {
	readonly rowCache: Map<number, DecodedRow>;
	readonly requestedChunks: Set<number>;
	readonly cacheGeneration: number;
	position: number | null;
	pendingOffset: number | null;
	inFlight: boolean;
	scrolling: boolean;
	settleTarget: number | null;
	gestureDistancePx: number;
	clearCache: () => void;
	isCacheGenerationCurrent: (generation: number) => boolean;
	applyDelta: (deltaLines: number, currentOffset: number, historySize: number) => number;
	/**
	 * Accumulate |dy| into the gesture-acceleration ramp and return the new cumulative
	 * distance. A direction reversal restarts the ramp from |dy| instead of adding to
	 * it — otherwise turning the gesture around would still be accelerated by distance
	 * traveled in the OPPOSITE direction, which is part of what made "scroll back
	 * accurately" hard.
	 */
	accumulateGesture: (dy: number) => number;
	snap: () => number | null;
	acceptSettledFrame: (displayOffset: number) => boolean;
	cancel: () => void;
}

export function createCanvasScrollController(): CanvasScrollController {
	const rowCache = new Map<number, DecodedRow>();
	const requestedChunks = new Set<number>();
	let position: number | null = null;
	let pendingOffset: number | null = null;
	let inFlight = false;
	let scrolling = false;
	let settleTarget: number | null = null;
	let gestureDistancePx = 0;
	let gestureSign = 0;
	let cacheGeneration = 0;

	return {
		rowCache,
		requestedChunks,
		get cacheGeneration() {
			return cacheGeneration;
		},
		get position() {
			return position;
		},
		set position(value) {
			position = value;
		},
		get pendingOffset() {
			return pendingOffset;
		},
		set pendingOffset(value) {
			pendingOffset = value;
		},
		get inFlight() {
			return inFlight;
		},
		set inFlight(value) {
			inFlight = value;
		},
		get scrolling() {
			return scrolling;
		},
		set scrolling(value) {
			scrolling = value;
		},
		get settleTarget() {
			return settleTarget;
		},
		set settleTarget(value) {
			settleTarget = value;
		},
		get gestureDistancePx() {
			return gestureDistancePx;
		},
		set gestureDistancePx(value) {
			gestureDistancePx = value;
		},
		clearCache() {
			cacheGeneration++;
			rowCache.clear();
			requestedChunks.clear();
		},
		isCacheGenerationCurrent(generation) {
			return generation === cacheGeneration;
		},
		applyDelta(deltaLines, currentOffset, historySize) {
			scrolling = true;
			const base = position ?? currentOffset;
			position = Math.max(0, Math.min(historySize, base - deltaLines));
			pendingOffset = Math.floor(position);
			return position;
		},
		accumulateGesture(dy) {
			const sign = dy > 0 ? 1 : dy < 0 ? -1 : 0;
			if (sign !== 0 && gestureSign !== 0 && sign !== gestureSign) gestureDistancePx = 0;
			if (sign !== 0) gestureSign = sign;
			gestureDistancePx += Math.abs(dy);
			return gestureDistancePx;
		},
		snap() {
			scrolling = false;
			gestureDistancePx = 0;
			gestureSign = 0;
			if (position === null) return null;
			const target = Math.round(position);
			position = target;
			pendingOffset = target;
			settleTarget = target;
			return target;
		},
		acceptSettledFrame(displayOffset) {
			if (settleTarget === null || displayOffset !== settleTarget) return false;
			settleTarget = null;
			position = null;
			return true;
		},
		cancel() {
			position = null;
			pendingOffset = null;
			scrolling = false;
			settleTarget = null;
			gestureDistancePx = 0;
			gestureSign = 0;
		},
	};
}
