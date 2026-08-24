/**
 * How terminal links activate:
 *  - "click": plain click opens (today's default behavior)
 *  - "modifier": only Cmd/Ctrl+click opens; underlines/pointer cursor only
 *    appear while the modifier is held (iTerm2-style reveal-on-hold)
 *  - "never": links never open on click; right-click → Open/Copy link is the
 *    only way to activate them. Dashed underlines stay visible so a link is
 *    still discoverable, but nothing hover-highlights or shows a pointer.
 */
export type LinkActivation = "click" | "modifier" | "never";

export interface LinkVisuals {
	/** Paint the always-on dashed underline for every detected link span. */
	dashed: boolean;
	/** Paint the solid underline for the currently hovered link. */
	solid: boolean;
	/** Show a pointer cursor and run hover resolution for the link under the mouse. */
	pointer: boolean;
}

/** Decide which link decorations to paint for the given mode/modifier state. */
export function linkVisuals(mode: LinkActivation, modifierHeld: boolean): LinkVisuals {
	switch (mode) {
		case "click":
			return { dashed: true, solid: true, pointer: true };
		case "modifier":
			return { dashed: modifierHeld, solid: modifierHeld, pointer: modifierHeld };
		case "never":
			return { dashed: true, solid: false, pointer: false };
	}
}

/**
 * Decide whether a click on a hovered link should open it. Reads the
 * modifier from the click event itself (the source of truth at activation
 * time), not the held-state signal used for painting.
 */
export function shouldOpenOnClick(mode: LinkActivation, modifierDown: boolean): boolean {
	switch (mode) {
		case "click":
			return true;
		case "modifier":
			return modifierDown;
		case "never":
			return false;
	}
}

/**
 * Decide whether a mousedown press over a detected link should be withheld
 * from the PTY (mouse-reporting) forward, so the app doesn't see a press the
 * user meant purely to activate the link:
 *  - A right-click (button 2) on a link is always withheld — it must reach
 *    the `contextmenu` handler for the Open/Copy-link menu, even while an app
 *    has mouse reporting on (WKWebView suppresses `contextmenu` if the
 *    mousedown's preventDefault fires; see #57).
 *  - A left-click (button 0) on a link is withheld only in "modifier" mode
 *    while the modifier is held — that combination is the click-to-open
 *    gesture. Ctrl is part of the SGR modifier bitmask, so on Windows/Linux
 *    an unguarded forward would otherwise also reach the TUI.
 *  - "click" mode's plain left-click-to-open is NOT withheld here: it opens
 *    via the `click` DOM event independently of whether this mousedown was
 *    forwarded, matching pre-existing behavior.
 */
export function shouldSkipMouseReportForLink(
	mode: LinkActivation,
	button: number,
	modifierHeld: boolean,
	onLink: boolean,
): boolean {
	if (!onLink) return false;
	if (button === 2) return true;
	return button === 0 && mode === "modifier" && modifierHeld;
}

/**
 * Whether a mousemove-driven hover check should run right now. Skips only in
 * "modifier" mode while the modifier isn't held — no point verifying a link
 * the user can't yet click. In "click" and "never" mode, detection keeps
 * running regardless of any modifier (needed for right-click resolution and,
 * in "never" mode, to keep the dashed underline/right-click menu working).
 */
export function shouldResolveLinkHoverOnMove(mode: LinkActivation, modifierHeld: boolean): boolean {
	return mode !== "modifier" || modifierHeld;
}

export interface LinkModifierEffectDecision {
	/** Clear any hovered link and reset the cursor — the modifier was released. */
	clearHover: boolean;
	/** Re-resolve hover at the last known mouse position — the modifier was just pressed. */
	recheckHover: boolean;
}

/**
 * Decide what a mode/modifier-change effect should do. Only "modifier" mode
 * reacts to the hold itself: releasing clears any stale hover so a solid
 * underline/pointer cursor can't linger past the key-up, and pressing
 * re-resolves hover at the last known mouse position (if one was recorded)
 * so the link under the cursor reveals without requiring the mouse to move.
 */
export function linkModifierEffectDecision(
	mode: LinkActivation,
	modifierHeld: boolean,
	hasHoverEvent: boolean,
): LinkModifierEffectDecision {
	const modifierRelevant = mode === "modifier";
	return {
		clearHover: modifierRelevant && !modifierHeld,
		recheckHover: modifierRelevant && modifierHeld && hasHoverEvent,
	};
}

export interface LinkSpan {
	colStart: number;
	colEnd: number;
}

export interface ResolvedRowLink {
	text: string;
	path: string;
	line?: number;
	col?: number;
	index: number;
}

export interface FileLinkCacheEntry {
	spans: LinkSpan[] | null;
	ts: number;
}

export interface CanvasLinkController {
	readonly rowCache: Map<string, ResolvedRowLink[] | null>;
	readonly fileCache: Map<string, FileLinkCacheEntry>;
	readonly detectedSpans: Map<number, LinkSpan[]>;
	readonly wrappedSpans: Map<number, LinkSpan[]>;
	beginCheck: () => number;
	isCurrent: (generation: number) => boolean;
	scheduleVerification: (verify: () => void | Promise<void>) => void;
	clearDetected: () => void;
	dispose: () => void;
}

export function createCanvasLinkController(delayMs = 150): CanvasLinkController {
	const rowCache = new Map<string, ResolvedRowLink[] | null>();
	const fileCache = new Map<string, FileLinkCacheEntry>();
	const detectedSpans = new Map<number, LinkSpan[]>();
	const wrappedSpans = new Map<number, LinkSpan[]>();
	let generation = 0;
	let verificationTimer: ReturnType<typeof setTimeout> | undefined;
	let disposed = false;

	return {
		rowCache,
		fileCache,
		detectedSpans,
		wrappedSpans,
		beginCheck() {
			return ++generation;
		},
		isCurrent(candidate) {
			return !disposed && candidate === generation;
		},
		scheduleVerification(verify) {
			if (disposed || verificationTimer !== undefined) return;
			verificationTimer = setTimeout(() => {
				verificationTimer = undefined;
				if (!disposed) void verify();
			}, delayMs);
		},
		clearDetected() {
			generation++;
			if (verificationTimer !== undefined) clearTimeout(verificationTimer);
			verificationTimer = undefined;
			detectedSpans.clear();
			wrappedSpans.clear();
		},
		dispose() {
			disposed = true;
			generation++;
			if (verificationTimer !== undefined) clearTimeout(verificationTimer);
			verificationTimer = undefined;
			rowCache.clear();
			fileCache.clear();
			detectedSpans.clear();
			wrappedSpans.clear();
		},
	};
}
