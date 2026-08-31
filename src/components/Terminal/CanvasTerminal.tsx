import { type Component, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { usePty } from "../../hooks/usePty";
import { lastMenuActionTime } from "../../menuDedup";
import { isLinkModifier, isMacOS, isWindows } from "../../platform";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { appLogger } from "../../stores/appLogger";
import { conversationStore } from "../../stores/conversationStore";
import { initLinkModifier, linkModifierHeld } from "../../stores/linkModifier";
import { settingsStore } from "../../stores/settings";
import { reconcileTerminalOwnership } from "../../stores/terminalOwnership";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { uiStore } from "../../stores/ui";
import { findBlockAtViewport, foldRange } from "../../utils/blockFold";
import { pickBlock } from "../../utils/blockNav";
import { filterMatchesToBlock, resolveScopedBlock } from "../../utils/blockSearchFilter";
import { writeClipboard } from "../../utils/clipboard";
import { formatRelativeTime } from "../../utils/formatRelativeTime";
import { ensureKeyboardViewportTracking, keyboardOcclusion } from "../../utils/keyboardViewport";
import { handleOpenUrl } from "../../utils/openUrl";
import { assignTabToActiveGroup } from "../../utils/paneTabAssign";
import { isPerfDebug } from "../../utils/perfDebug";
import { markPerf, noteFrameRequest } from "../../utils/perfTrace";
import { getShellFamily, sendCommand, shouldAutoSubmitSuggestion } from "../../utils/sendCommand";
import { switchToTerminalBySession } from "../../utils/switchToTerminalBySession";
import { applyPinchFontDelta } from "../../utils/terminalZoom";
import { ContextMenu, createContextMenu } from "../ContextMenu/ContextMenu";
import { createCanvasTerminalBindings } from "./canvasTerminalBindings";
import { type ClickCounterState, classifyClick, decideMousedownSelection } from "./canvasTerminalGestures";
import { canToggleFold, gutterMarkKind, gutterZoneAt } from "./canvasTerminalGutter";
import {
	createCanvasLinkController,
	linkModifierEffectDecision,
	linkVisuals,
	shouldOpenOnClick,
	shouldResolveLinkHoverOnMove,
	shouldSkipMouseReportForLink,
} from "./canvasTerminalLinks";
import {
	type ScrollbarMarksInput,
	scrollbarMarksHtml,
	scrollbarMarksKey,
	shouldShowScrollbar,
} from "./canvasTerminalMarks";
import { createCanvasScrollController, gestureAccelFactor, ROW_CACHE_CHUNK } from "./canvasTerminalScroll";
import {
	buildSmartSelectionWindow,
	createCanvasSearchController,
	createCanvasSelectionController,
	createWordBoundaryResolver,
	extendSelectionDrag,
	type SelectionPoint,
	type WordBoundaryFn,
	wordBoundsAt,
} from "./canvasTerminalSelection";
import { installTouchHandlers } from "./canvasTerminalTouch";
import { createTransport, type TerminalTransport, toBinaryPayload } from "./canvasTerminalTransport";
import {
	type CellMetrics,
	type CursorShape,
	clampRowRangeToViewport,
	computeCursorRect,
	createHiddenAckThrottle,
	createLeadingThrottle,
	type DecodedFrame,
	type DecodedRow,
	decideFrameGrid,
	decodeBinaryFrame,
	decodeStyledRange,
	GUTTER_PX,
	gridDimsForBox,
	isWideCursorGlyph,
	lastGridCol,
	motionReportButton,
	reconcileDelay,
	resolveCursorShape,
	rowText,
	shouldFireReconcile,
	shouldForwardMouseGesture,
	shouldPaintCursor,
	snapLineHeight,
} from "./canvasTerminalUtils";
import {
	createWheelNotchState,
	quantizeWheelNotches,
	resetWheelNotch,
	WHEEL_GESTURE_END_MS,
	wheelDeltaToPixels,
} from "./canvasTerminalWheel";
import { installFrameTimingDebugHook, isFrameTimingEnabled, recordFrameTiming, resetFrameTiming } from "./frameTiming";
import { acquireCache, getSharedMetrics, invalidateGlyphCache, releaseCache } from "./glyphCache";
import { createGridRenderer, type GridRenderer } from "./gridRenderer";
import { kittySequenceForKey } from "./kittyKeyboard";
import { filePathRegex, fileUrlRegex, matchWebUrls } from "./linkProvider";
import { findSmartMatch, SMART_SELECTION_RADIUS, type SmartMatch } from "./smartSelection";
import { runSmartSelectionAction, type SmartSelectionActionDeps } from "./smartSelectionActions";
import { resolveSmartSelectionRules } from "./smartSelectionDefaults";
import type { SmartSelectionAction as SmartSelectionActionType } from "./smartSelectionTypes";
import { INTENT_HIGHLIGHT_RE, planSuggestOverlay, SUGGEST_ANCHOR_RE } from "./suggestOverlay";
import {
	altSequenceFromCode,
	createCompositionState,
	isGlobalShortcutPassthrough,
	isPointerInsideRect,
	keyToSequence,
	shouldReportMouseUp,
} from "./terminalInput";

// Re-export for external consumers
export type { CellMetrics, CursorShape, DecodedFrame };

export interface CanvasTerminalRef {
	focus: () => void;
	refresh: () => void;
	resubscribe: () => Promise<void>;
	getSelectionText: () => string;
	searchFind: (query: string, blockScope?: boolean) => Promise<{ index: number; count: number }>;
	searchNext: () => { index: number; count: number };
	searchPrev: () => { index: number; count: number };
	searchClear: () => void;
	/** Paste text with correct bracketed paste wrapping based on current terminal state */
	paste: (text: string) => void;
	/** Scroll the viewport to the nearest command block boundary in `direction`. */
	scrollToBlock: (direction: "previous" | "next") => void;
	/** Toggle fold on the command block nearest the viewport's vertical center. */
	toggleBlockFoldAtViewport: () => void;
}

export interface CanvasTerminalProps {
	sessionId: string;
	terminalId: string;
	onOpenFilePath?: (path: string, line?: number, col?: number) => void;
	onSearchOpen?: () => void;
	onSearchClose?: () => void;
	searchVisible?: boolean;
	onResume?: () => void;
	onResumeDismiss?: () => void;
	hasPendingResume?: boolean;
	onCwdChange?: (id: string, cwd: string) => void;
	onFocus?: () => void;
	onRef?: (ref: CanvasTerminalRef) => void;
	onBell?: () => void;
}

/** Shortest gap between two runs of the open search.
 *  A redrawing TUI emits frames far faster than a regex sweep over the whole
 *  scrollback is worth running, so the sweep is bounded to one per window —
 *  bounded, not postponed: waiting for the frames to stop meant never running. */
const SEARCH_REFRESH_THROTTLE_MS = 150;

const CanvasTerminal: Component<CanvasTerminalProps> = (props) => {
	let canvasRef!: HTMLCanvasElement;
	let overlayCanvasRef!: HTMLCanvasElement;
	let touchTextareaRef!: HTMLTextAreaElement;
	let keyInputRef!: HTMLInputElement;
	let scrollbarRef!: HTMLDivElement;
	let scrollThumbRef!: HTMLDivElement;
	let overlayRef!: HTMLDivElement;
	let containerRef!: HTMLDivElement;
	// Smooth-scroll stage: wraps base + overlay canvases and gets a transient
	// translateY during a scroll gesture (snaps back to 0 on a line boundary).
	let stageRef!: HTMLDivElement;
	// Wraps the stage; gets a translateY to slide ONLY this terminal up so the
	// cursor stays visible above the on-screen keyboard on touch devices. Clipped
	// by containerRef's overflow:hidden; independent of the stage's scroll transform.
	let kbLiftRef!: HTMLDivElement;
	// Behind the base canvas: paints only the one row above and one below the
	// viewport, revealed as the stage slides. Never used for hit-testing.
	let overscanCanvasRef!: HTMLCanvasElement;
	let ctx!: CanvasRenderingContext2D;
	let octx!: CanvasRenderingContext2D;
	let octxOverscan: CanvasRenderingContext2D | null = null;
	let overscanRenderer: GridRenderer | null = null;
	// Client-side styled-row cache for smooth scroll, keyed by the backend's
	// eviction-stable absolute row index (`historyBase + grid-relative`, where
	// historyBase counts lines already dropped from the history top). A physical line
	// keeps its key for life — even once the scrollback cap rotates — so a cached row
	// can never alias onto a different line and ghost/duplicate during a scroll.
	// `requestedChunks` dedupes background range prefetches.
	const scroll = createCanvasScrollController();
	const rowCache = scroll.rowCache;
	const requestedChunks = scroll.requestedChunks;
	// Per-gesture pixel accumulator for the app-forwarded wheel path (handleWheel below).
	// Lives for the pane's lifetime; reset on gesture-end, direction reversal, blur, and
	// primary/alternate screen swap — see the reset points next to its usages.
	const wheelNotch = createWheelNotchState();
	// Base-grid renderer (the canvas2d paint implementation). Created in onMount
	// once ctx exists.
	let gridRenderer!: GridRenderer;

	const [metrics, setMetrics] = createSignal<CellMetrics | null>(null);
	const [focused, setFocused] = createSignal(false);
	const isTouchDevice = navigator.maxTouchPoints > 0 || "ontouchstart" in window;
	let currentFrame: DecodedFrame | null = null;
	let lastDisplayOffset = -1;
	let lastAltScreen = false;
	let screenGeneration = 0;
	let lastScreenRows = -1;
	let lastScreenCols = -1;
	const search = createCanvasSearchController();
	// Live search query, kept so incoming frames can re-run it. Search matches are
	// anchored to ABSOLUTE rows, but a TUI (ink agents, vim, lazygit) rewrites the
	// live screen rows in place: the text under a highlight changes while the
	// highlight stays pinned, painting an orange box over cells that no longer
	// match. Rewritten rows drop their matches immediately; this re-search restores
	// the ones that still hit.
	let searchQuery = "";
	let searchBlockScope = false;
	/** The block "Search in Block" is actually scoped to, for `paintSearchScopeIndicator`
	 *  (issue #4) — resolved silently before this, with nothing telling the user which
	 *  block a block-scoped search landed on. */
	let scopedSearchBlock: import("../../utils/blockSearchFilter").BlockRange | null = null;
	/** Leading-edge, so a continuously redrawing TUI still refreshes the search.
	 *  A trailing debounce reset its timer on every frame and so never fired. */
	const searchRefresh = createLeadingThrottle(() => {
		// Nobody awaits this one, so it needs its own catch: the backend read
		// can reject (a closed session over HTTP, a failed blocking-pool task),
		// and an unhandled rejection from a timer is invisible until it isn't.
		// The matches simply stay as they were until the next frame retriggers.
		runSearchQuery(false).catch((e) => {
			appLogger.warn("terminal", "search refresh failed", { error: String(e) });
		});
	}, SEARCH_REFRESH_THROTTLE_MS);
	let cursorBlinkOn = true;
	let blinkInterval: ReturnType<typeof setInterval> | undefined;
	let blinkResetAt = 0;
	let unsubscribe: (() => void) | undefined;
	let resizeObserver: ResizeObserver | undefined;
	let visibilityObserver: IntersectionObserver | undefined;
	let lastResizeCols = 0;
	let lastResizeRows = 0;
	// Scrollbar track height (= canvas logical height), cached at remeasure so the
	// per-frame scroll path never reads scrollbarRef.clientHeight (a layout-forcing
	// read — same class as the documented getBoundingClientRect-per-frame P1).
	let scrollbarTrackHeight = 0;
	let transport: TerminalTransport | undefined;
	let invokeRef: ((cmd: string, args: Record<string, unknown>) => Promise<unknown>) | undefined;
	let rafId: number | undefined;
	// Render-scheduling stamp: when a repaint is first requested, the gap to the rAF
	// callback is the "sched" metric — scheduling latency under CPU load (0 = no
	// pending request). Gated by isFrameTimingEnabled().
	let mainDirtySince = 0;
	let resizeDebounce: ReturnType<typeof setTimeout> | undefined;
	let dprMediaQuery: MediaQueryList | undefined;
	let dprChangeHandler: (() => void) | undefined;
	let cleanupTouch: (() => void) | undefined;
	let alive = true;
	const bindings = createCanvasTerminalBindings();
	const ipcErr = (cmd: string) => (e: unknown) =>
		appLogger.debug("terminal", `${cmd} failed`, { sessionId: props.sessionId, error: e });

	// Selection state — row coordinates are absolute (viewportTop + viewportRow)
	// so the highlight stays anchored to the original content when scrolling.
	const selection = createCanvasSelectionController();
	let selectionScrollTimer: ReturnType<typeof setInterval> | null = null;
	let selectionScrollDelta = 0;
	// Selection-drag coalescing (#9b13): cache the canvas rect once per drag and
	// collapse the repaint burst into one paint per frame via rAF — mirrors the
	// scroll/resize coalescing so raw mousemoves don't force a gBCR + full paint each.
	let selectionDragRect: DOMRect | null = null;
	let selectionRafId: number | undefined;
	let lastSelectionEvent: MouseEvent | null = null;

	// Link detection
	const linkController = createCanvasLinkController();
	const linkCache = linkController.rowCache;
	let hoveredLink: {
		row: number;
		colStart: number;
		colEnd: number;
		path: string;
		line?: number;
		col?: number;
		spans?: { row: number; colStart: number; colEnd: number }[];
	} | null = null;
	const detectedLinks = linkController.detectedSpans;
	// Spans of links that span soft-wrapped rows (web + file://), keyed by row.
	// scanRowForLinks() merges these each time it rebuilds a row's dashed-underline
	// spans, so multi-row underlines survive the per-frame rebuild at line ~1367.
	// Recomputed by verifyVisibleFileLinks(); cleared with detectedLinks so it
	// can't paint stale rows after a scroll/resize.
	const wrappedLinkSpans = linkController.wrappedSpans;
	function clearDetectedLinks() {
		linkController.clearDetected();
	}
	// Last document-level mousemove seen while checking link hover — replayed by
	// the modifier-held effect below so pressing Cmd/Ctrl reveals the link under
	// the cursor immediately, without requiring the mouse to move.
	let lastLinkHoverEvent: MouseEvent | null = null;
	/** Which link decorations (dashed/solid underline, pointer cursor) apply right now. */
	function currentLinkVisuals() {
		return linkVisuals(settingsStore.state.linkActivation, linkModifierHeld());
	}

	// Link context menu: right-clicking a detected link offers Open / Copy link.
	// TUIC is UI-first — opening a link (e.g. a markdown file) is a primary action,
	// so plain left-click still opens; this menu adds a non-destructive way to copy
	// the target without opening it.
	type LinkTarget = { path: string; line?: number; col?: number };
	const linkMenu = createContextMenu();
	const [linkMenuTarget, setLinkMenuTarget] = createSignal<LinkTarget | null>(null);
	/** Smart-selection rule match under the last right-click, if any — its
	 *  actions are merged into the same link-menu popup (see the JSX below
	 *  and the "contextmenu" listener). Cleared whenever the menu closes so a
	 *  stale rule's actions can't linger into an unrelated right-click. */
	const [smartMenuMatch, setSmartMenuMatch] = createSignal<ResolvedSmartMatch | null>(null);

	const openLink = (link: LinkTarget) => {
		if (link.path.startsWith("http://") || link.path.startsWith("https://")) {
			handleOpenUrl(link.path);
		} else {
			const path = link.path.startsWith("file://") ? link.path.slice(7) : link.path;
			props.onOpenFilePath?.(path, link.line, link.col);
		}
	};

	const copyLink = (link: LinkTarget) => {
		const text = link.path.startsWith("file://") ? link.path.slice(7) : link.path;
		writeClipboard(text).catch(() => {});
	};

	const pty = usePty();

	function currentTerminalCwd(): string {
		const termId = terminalsStore.getTerminalForSession(props.sessionId);
		return (termId ? terminalsStore.get(termId)?.cwd : null) ?? "";
	}

	/** Real `SmartSelectionActionDeps` for this terminal — each a thin wrapper
	 *  around an existing utility, bound to the current session. Built fresh
	 *  per dispatch (cheap closures, not a hot path). */
	function smartSelectionActionDeps(): SmartSelectionActionDeps {
		return {
			copyToClipboard: (text) => writeClipboard(text),
			openUrl: handleOpenUrl,
			openFile: (pathSpec) => {
				// Optional trailing ":line" / ":line:col" — e.g. the dev-file-line-col rule's match.
				const parsed = /^(.+?)(?::(\d+))?(?::(\d+))?$/.exec(pathSpec);
				const path = parsed?.[1] ?? pathSpec;
				const line = parsed?.[2] ? Number(parsed[2]) : undefined;
				const col = parsed?.[3] ? Number(parsed[3]) : undefined;
				props.onOpenFilePath?.(path, line, col);
			},
			sendText: async (text, autoSubmitAllowed) => {
				const agentType = terminalsStore.getAgentTypeForSession(props.sessionId);
				const shellFamily = await getShellFamily(props.sessionId);
				const submit = autoSubmitAllowed && shouldAutoSubmitSuggestion(agentType, text);
				await sendCommand(
					async (data) => {
						await invokeRef?.("write_pty", { sessionId: props.sessionId, data });
					},
					text,
					agentType,
					shellFamily,
					submit,
				);
			},
			runInNewTerminal: async (text) => {
				const canSpawn = await pty.canSpawn();
				if (!canSpawn) {
					toastsStore.add("Can't run in a new terminal", "Max sessions reached (50)", "warn");
					return;
				}
				const id = terminalsStore.add({
					sessionId: null,
					fontSize: settingsStore.state.defaultFontSize,
					name: terminalsStore.nextDefaultName(),
					cwd: currentTerminalCwd() || null,
					awaitingInput: null,
				});
				assignTabToActiveGroup(id, "terminal");
				terminalsStore.update(id, { pendingInitCommand: text });
				terminalsStore.setActive(id);
			},
			askAi: (text) => {
				uiStore.setAiChatPanelVisible(true);
				switchToTerminalBySession(props.sessionId);
				conversationStore.sendMessage(text, props.sessionId);
			},
			onBlockedUrl: (url) => {
				toastsStore.add("Blocked URL", `Disallowed scheme: ${url.slice(0, 80)}`, "warn");
			},
		};
	}

	/** A `SmartMatch` with its text-offset span already mapped to grid coordinates. */
	type ResolvedSmartMatch = SmartMatch & { startCoord: SelectionPoint; endCoord: SelectionPoint };

	/** Dispatch a smart-selection rule's action — Alt+double-click's default
	 *  action, or a context-menu item for any of a matched rule's actions. */
	function runSmartAction(action: SmartSelectionActionType, match: ResolvedSmartMatch): void {
		runSmartSelectionAction(
			action,
			{
				matchText: match.text,
				groups: match.groups,
				cwd: currentTerminalCwd(),
				user: undefined,
				host: undefined,
			},
			smartSelectionActionDeps(),
		).catch((error) => appLogger.warn("terminal", "Smart selection action failed", { error }));
	}

	// Cached CSS custom properties (re-read on remeasure, not every frame)
	let cachedBgDefault = "#1e1e1e";
	let cachedFgDefault = "#d4d4d4";

	// Tracks cumulative gesture distance (px) to ramp the scroll acceleration factor.
	// Row index → row data lookup (persistent, updated incrementally)
	const rowMap = new Map<number, DecodedFrame["rows"][0]>();
	// Rows that arrived in the latest onFrame batch (drives incremental repaint)
	const pendingDirtyRows = new Set<number>();
	// When true, next paint must redraw everything (scroll, resize, clear)
	let fullRepaintNeeded = true;
	let hidden = false;
	let lastHistorySize = -1;
	// How many grid frames this transport has delivered. The ack echoes it so Rust
	// can tell "the frontend caught up" from "a late ack for a frame the ticker
	// already gave up on" — without an id, the second one reopened the gate and
	// sent a burst at exactly the moment the frontend was behind. Reset with the
	// channel: (re)subscribing gives Rust a fresh gate counting from zero.
	let framesReceived = 0;

	function ackFrame() {
		transport?.ackFrame(framesReceived);
	}

	// Below the backend's MAX_IN_FLIGHT_MS (500 ms): the gate must reopen on this
	// ack, not on the ticker deciding the frontend is stuck.
	const hiddenAck = createHiddenAckThrottle(ackFrame, 400);

	function writePtyNoScroll(data: string) {
		invokeRef?.("write_pty", { sessionId: props.sessionId, data }).catch((e) => {
			appLogger.warn("terminal", "PTY write failed", { sessionId: props.sessionId, error: e });
		});
	}

	function writePty(data: string) {
		// Typing jumps to the bottom — abandon any in-flight smooth scroll gesture so
		// its transient transform/cache render doesn't fight the programmatic jump.
		resetSmoothScroll();
		if (currentFrame && currentFrame.displayOffset > 0) {
			invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta: -currentFrame.displayOffset }).catch(
				ipcErr("terminal_scroll"),
			);
		}
		writePtyNoScroll(data);
	}

	function scheduleRepaint() {
		// A smooth-scroll gesture owns the base canvas (rendered locally from cache);
		// don't let backend-frame repaints fight it until the gesture settles.
		if (scroll.position != null) {
			return;
		}
		if (rafId !== undefined) return;
		if (hidden) {
			return;
		}
		if (!alive) return;
		// Stamp the first repaint request of this cycle ("sched" — see decl).
		if (mainDirtySince === 0 && isFrameTimingEnabled()) {
			mainDirtySince = performance.now();
		}
		rafId = requestAnimationFrame(() => {
			rafId = undefined;
			if (!alive || hidden) return;
			const m = metrics();
			if (currentFrame && m) {
				const dirty = pendingDirtyRows.size > 0 ? new Set(pendingDirtyRows) : undefined;
				pendingDirtyRows.clear();
				const timing = isFrameTimingEnabled();
				// "sched": request->rAF-callback delay — scheduling latency / vsync rAF priority.
				if (timing && mainDirtySince) {
					recordFrameTiming(props.sessionId, "sched", performance.now() - mainDirtySince);
				}
				// "paint": the base grid paint cost.
				const paintT0 = timing ? performance.now() : 0;
				paintFrame(currentFrame, m, dirty);
				if (timing) recordFrameTiming(props.sessionId, "paint", performance.now() - paintT0);
			}
			mainDirtySince = 0;
		});
	}

	function startSelectionScroll(delta: number) {
		if (selectionScrollTimer !== null && selectionScrollDelta === delta) return;
		stopSelectionScroll();
		selectionScrollDelta = delta;
		const speed = Math.min(Math.abs(delta), 5);
		const interval = Math.max(20, 80 - speed * 12);
		selectionScrollTimer = setInterval(() => {
			if (!selection.selecting || !selection.start || !currentFrame || !invokeRef) {
				stopSelectionScroll();
				return;
			}
			const scrollDir = delta > 0 ? 1 : -1;
			invokeRef("terminal_scroll", { sessionId: props.sessionId, delta: scrollDir }).catch(ipcErr("terminal_scroll"));
			const edgeRow = scrollDir > 0 ? 0 : (currentFrame.screenRows || lastResizeRows) - 1;
			const absRow = viewportRowToAbs(edgeRow);
			if (absRow !== null) {
				selection.end = { col: scrollDir > 0 ? 0 : 9999, row: absRow + scrollDir };
				const m = metrics();
				if (m) paintFrame(currentFrame, m);
			}
		}, interval);
	}

	function stopSelectionScroll() {
		if (selectionScrollTimer !== null) {
			clearInterval(selectionScrollTimer);
			selectionScrollTimer = null;
			selectionScrollDelta = 0;
		}
	}

	function canvasToGrid(e: MouseEvent, cachedRect?: DOMRect): { col: number; row: number } {
		const m = metrics();
		if (!m) return { col: 0, row: 0 };
		const rect = cachedRect ?? canvasRef.getBoundingClientRect();
		const x = e.clientX - rect.left - GUTTER_PX;
		const y = e.clientY - rect.top;
		const maxCol = lastGridCol(rect.width, m.cellWidth);
		const maxRow = Math.max(0, Math.floor(rect.height / m.cellHeight) - 1);
		return {
			col: Math.max(0, Math.min(Math.floor(x / m.cellWidth), maxCol)),
			row: Math.max(0, Math.min(Math.floor(y / m.cellHeight), maxRow)),
		};
	}

	/** Last selectable column of a line — shared by triple-click and line-mode
	 *  drag extension so both agree with canvasToGrid's own maxCol (see
	 *  lastGridCol's own doc comment in canvasTerminalUtils.ts). */
	function lastGridColForRect(rect: DOMRect): number {
		const m = metrics();
		if (!m) return 79;
		return lastGridCol(rect.width, m.cellWidth);
	}

	function mouseModifiers(e: MouseEvent): number {
		return (e.shiftKey ? 4 : 0) | (e.altKey ? 8 : 0) | (e.ctrlKey ? 16 : 0);
	}

	function sgrMouseSequence(button: number, col: number, row: number, press: boolean, e?: MouseEvent): string {
		const cb = button + (e ? mouseModifiers(e) : 0);
		return `\x1b[<${cb};${col + 1};${row + 1}${press ? "M" : "m"}`;
	}

	function viewportRowToAbs(viewportRow: number): number | null {
		if (!currentFrame) return null;
		return currentFrame.historySize - currentFrame.displayOffset + viewportRow;
	}

	function remeasure() {
		if (!ctx) return;
		const rect = containerRef.getBoundingClientRect();
		if (rect.width <= 0 || rect.height <= 0) return;

		const dpr = window.devicePixelRatio || 1;
		const perTerminalSize = terminalsStore.state.terminals[props.terminalId]?.fontSize;
		const fontSize = perTerminalSize ?? settingsStore.state.defaultFontSize;
		const fontFamily = settingsStore.getFontFamily();
		const fontWeight = settingsStore.state.fontWeight;
		const m = getSharedMetrics(fontSize, fontFamily, dpr, snapLineHeight(fontSize), fontWeight);
		setMetrics(m);

		cachedBgDefault = getComputedStyle(canvasRef).getPropertyValue("--bg-secondary").trim() || "#1e1e1e";
		cachedFgDefault = getComputedStyle(canvasRef).getPropertyValue("--fg-primary").trim() || "#d4d4d4";
		gridRenderer.setTheme(cachedBgDefault, cachedFgDefault);

		const { rows, cols } = gridDimsForBox(rect.width, rect.height, m.cellWidth, m.cellHeight);
		if (cols <= 0 || rows <= 0) return;
		// A resize invalidates the smooth-scroll geometry (cell metrics, overscan,
		// row cache). Cancel any in-flight gesture so the new geometry takes over
		// cleanly. Cheap no-op when no gesture is active (scrollPosF already null).
		resetSmoothScroll();
		const logicalW = cols * m.cellWidth + GUTTER_PX;
		const logicalH = rows * m.cellHeight;
		// Cache the scrollbar track height here (resize time) so the per-frame path
		// uses this instead of reading scrollbarRef.clientHeight every frame.
		scrollbarTrackHeight = logicalH;
		canvasRef.width = logicalW * dpr;
		canvasRef.height = logicalH * dpr;
		ctx.scale(dpr, dpr);
		ctx.translate(GUTTER_PX, 0);
		canvasRef.style.width = `${logicalW}px`;
		canvasRef.style.height = `${logicalH}px`;
		overlayCanvasRef.width = logicalW * dpr;
		overlayCanvasRef.height = logicalH * dpr;
		overlayCanvasRef.style.width = `${logicalW}px`;
		overlayCanvasRef.style.height = `${logicalH}px`;
		octx.scale(dpr, dpr);
		octx.translate(GUTTER_PX, 0);

		// Overscan canvas (smooth scroll): one extra row above and below the viewport.
		// Positioned -cellHeight so its drawing y=0 maps to the row just above the
		// viewport; the row below is drawn at (rows+1)*cellHeight.
		if (overscanCanvasRef) {
			const overscanH = logicalH + 2 * m.cellHeight;
			overscanCanvasRef.width = logicalW * dpr;
			overscanCanvasRef.height = overscanH * dpr;
			overscanCanvasRef.style.width = `${logicalW}px`;
			overscanCanvasRef.style.height = `${overscanH}px`;
			overscanCanvasRef.style.top = `${-m.cellHeight}px`;
			if (!octxOverscan) {
				octxOverscan = overscanCanvasRef.getContext("2d", { alpha: true });
				if (octxOverscan) {
					overscanRenderer = createGridRenderer(octxOverscan, {
						fontWeight: () => settingsStore.state.fontWeight,
						getFontFamily: () => settingsStore.getFontFamily(),
					});
				}
			}
			if (octxOverscan && overscanRenderer) {
				octxOverscan.setTransform(1, 0, 0, 1, 0, 0);
				octxOverscan.scale(dpr, dpr);
				octxOverscan.translate(GUTTER_PX, 0);
				overscanRenderer.setTheme(cachedBgDefault, cachedFgDefault);
			}
			scroll.clearCache();
		}
		if (
			cols > 0 &&
			rows > 0 &&
			logicalW > 0 &&
			logicalH > 0 &&
			invokeRef &&
			(cols !== lastResizeCols || rows !== lastResizeRows)
		) {
			lastResizeCols = cols;
			lastResizeRows = rows;

			rowMap.clear();
			clearDetectedLinks();
			fullRepaintNeeded = true;
			lastDisplayOffset = -1;
			invokeRef("resize_pty", { sessionId: props.sessionId, rows, cols }).catch(ipcErr("resize_pty"));
		}

		if (currentFrame) {
			fullRepaintNeeded = true;
			paintFrame(currentFrame, m);
		}
	}

	function paintFrame(frame: DecodedFrame, m: CellMetrics, dirtyIndices?: Set<number>) {
		gridRenderer.paintGrid(rowMap, m, { fullRepaint: fullRepaintNeeded, dirtyIndices });
		fullRepaintNeeded = false;

		// Overlay (cursor/selection/search/links/scrollbar/suggest) always stays on main.
		repaintOverlay(frame, m);

		updateScrollbar(frame);
		updateSuggestOverlay(frame, m, dirtyIndices);
	}

	function repaintOverlay(frame: DecodedFrame, m: CellMetrics) {
		octx.clearRect(-GUTTER_PX, 0, overlayCanvasRef.width / m.dpr, overlayCanvasRef.height / m.dpr);
		paintSelection(m);
		paintSearchHighlights(m);
		paintSearchScopeIndicator(m);
		paintLinkUnderline(frame, m);
		paintGutterMarkers(m);
		paintFoldChevrons(m);
		paintBlockTimestamps(m);
		paintFoldedBlocks(m);
		paintCursor(frame, m);
	}

	function paintLinkUnderline(_frame: DecodedFrame, m: CellMetrics) {
		const maxRow = currentFrame?.screenRows || lastResizeRows;
		const visuals = currentLinkVisuals();
		octx.strokeStyle = cachedFgDefault;
		octx.lineWidth = 1;

		// Dashed underline for all detected links
		if (visuals.dashed && detectedLinks.size > 0) {
			octx.globalAlpha = 0.4;
			octx.setLineDash([2, 3]);
			octx.beginPath();
			for (const [row, spans] of detectedLinks) {
				if (row < 0 || row >= maxRow) continue;
				const y = row * m.cellHeight + m.cellHeight - 1 + 0.5;
				for (const span of spans) {
					octx.moveTo(span.colStart * m.cellWidth, y);
					octx.lineTo(span.colEnd * m.cellWidth, y);
				}
			}
			octx.stroke();
			octx.setLineDash([]);
			octx.globalAlpha = 1;
		}

		// Solid underline for hovered link
		if (visuals.solid && hoveredLink) {
			const rowSpans = hoveredLink.spans || [
				{ row: hoveredLink.row, colStart: hoveredLink.colStart, colEnd: hoveredLink.colEnd },
			];
			for (const span of rowSpans) {
				if (span.row >= 0 && span.row < maxRow) {
					const x = span.colStart * m.cellWidth;
					const w = (span.colEnd - span.colStart) * m.cellWidth;
					const y = span.row * m.cellHeight + m.cellHeight - 1 + 0.5;
					octx.beginPath();
					octx.moveTo(x, y);
					octx.lineTo(x + w, y);
					octx.stroke();
				}
			}
		}
	}

	function scrollToMatch(match: { row: number; col_start: number; col_end: number }) {
		if (!currentFrame || !invokeRef) return;
		const viewportTop = currentFrame.historySize - currentFrame.displayOffset;
		const screenLines = currentFrame.screenRows || lastResizeRows;
		const viewportBottom = viewportTop + screenLines;
		if (match.row >= viewportTop && match.row < viewportBottom) return;
		const targetOffset = currentFrame.historySize - match.row + Math.floor(screenLines / 2);
		const clamped = Math.max(0, Math.min(targetOffset, currentFrame.historySize));
		const delta = clamped - currentFrame.displayOffset;
		if (delta !== 0) {
			invokeRef("terminal_scroll", { sessionId: props.sessionId, delta }).catch(ipcErr("terminal_scroll"));
		}
	}

	/** Run `searchQuery` against the backend grid and adopt the result.
	 *
	 *  Shared by the `searchFind` ref method and by the frame-driven refresh, so a
	 *  live TUI redraw re-derives matches through exactly the same path as a fresh
	 *  find — no second copy of the block-scope / viewport-anchoring rules. */
	async function runSearchQuery(
		scrollToActive: boolean,
		preserveActiveByValue = true,
	): Promise<{ index: number; count: number }> {
		if (!searchQuery || !invokeRef) {
			search.clear();
			return { index: -1, count: 0 };
		}
		const generation = screenGeneration;
		const query = searchQuery;
		let matches = (await invokeRef("terminal_search", {
			sessionId: props.sessionId,
			query,
		})) as { row: number; col_start: number; col_end: number }[];
		// The query can be cleared or replaced while the sweep is in flight, and
		// neither bumps `screenGeneration`. Cancelling the refresh throttle stops
		// the timer, not a request already out — so without this the stale answer
		// resurrects highlights the user just cleared, or overwrites the newer
		// query's matches with the older query's.
		if (generation !== screenGeneration || query !== searchQuery) return { index: -1, count: 0 };
		scopedSearchBlock = null;
		if (searchBlockScope && currentFrame) {
			const term = terminalsStore.get(props.terminalId);
			if (term) {
				const allBlocks = term.activeBlock ? [...term.commandBlocks, term.activeBlock] : term.commandBlocks;
				const viewTop = currentFrame.historySize - currentFrame.displayOffset;
				const viewCenter = viewTop + Math.floor(currentFrame.screenRows / 2);
				scopedSearchBlock = resolveScopedBlock(allBlocks, viewCenter) ?? null;
				matches = filterMatchesToBlock(matches, allBlocks, viewCenter);
			}
		}
		const activeMatch = search.replace(
			matches,
			currentFrame
				? {
						historySize: currentFrame.historySize,
						displayOffset: currentFrame.displayOffset,
						screenRows: currentFrame.screenRows || lastResizeRows,
					}
				: undefined,
			preserveActiveByValue,
		);
		// Only a user-driven find/next/prev may move the viewport. A refresh triggered
		// by the agent repainting must never yank the screen out from under the user.
		if (scrollToActive && activeMatch) scrollToMatch(activeMatch);
		const m = metrics();
		if (currentFrame && m) paintFrame(currentFrame, m);
		return { index: search.activeIndex, count: matches.length };
	}

	/** Coalesce the re-search: a redrawing TUI emits frames far faster than a
	 *  regex sweep over the whole scrollback is worth running. */
	function scheduleSearchRefresh(): void {
		if (!searchQuery) return;
		searchRefresh.trigger();
	}

	function clearSearchState(): void {
		searchQuery = "";
		searchRefresh.cancel();
		scopedSearchBlock = null;
		search.clear();
	}

	function absRowToViewport(absRow: number): number | null {
		if (!currentFrame) return null;
		// During a smooth-scroll gesture the base is cache-rendered at overlayScrollOffset
		// (the backend frame lags), so map overlay rows against that same offset.
		const offset = overlayScrollOffset ?? currentFrame.displayOffset;
		const viewportTop = currentFrame.historySize - offset;
		const viewportRow = absRow - viewportTop;
		if (viewportRow < 0 || viewportRow >= (currentFrame.screenRows || lastResizeRows)) return null;
		return viewportRow;
	}

	/**
	 * Clamps an absolute-row range `[startRow, endRow)` to the rows currently in
	 * the viewport, returning viewport-relative start/end (inclusive), or `null`
	 * if the range doesn't overlap the viewport at all. A multi-row range needs
	 * this rather than calling `absRowToViewport` on each edge separately: when
	 * BOTH edges have scrolled past the viewport (one above, one below — a fold
	 * or search-scoped block taller than the screen), each edge individually
	 * maps to `null`, which is indistinguishable from "this range doesn't
	 * overlap the viewport at all" unless the caller checks the overlap itself
	 * first. Two real bugs shipped from exactly that ambiguity: `paintFoldedBlocks`
	 * skipped drawing its opaque hide-rect entirely whenever the fold's start
	 * row scrolled off the top (even with part of the fold still visible below —
	 * making the "hide the text" fix from issue #1 leave it fully visible in that
	 * scroll position), and `paintSearchScopeIndicator` used to fall back to
	 * `?? 0`/`?? lastResizeRows - 1` unconditionally, drawing a full-viewport-height
	 * bar once the scoped block scrolled fully off-screen in either direction.
	 */
	/** Same offset math as `absRowToViewport` (including the smooth-scroll
	 *  `overlayScrollOffset` override) — the shared bounds callers of
	 *  `clampRowRangeToViewport` need. */
	function currentViewportBounds(): { viewTop: number; viewBottom: number } | null {
		if (!currentFrame) return null;
		const offset = overlayScrollOffset ?? currentFrame.displayOffset;
		const viewTop = currentFrame.historySize - offset;
		const viewBottom = viewTop + (currentFrame.screenRows || lastResizeRows);
		return { viewTop, viewBottom };
	}

	function paintSearchHighlights(m: CellMetrics) {
		if (search.matches.length === 0) return;
		for (let i = 0; i < search.matches.length; i++) {
			const match = search.matches[i];
			const vpRow = absRowToViewport(match.row);
			if (vpRow === null) continue;
			const isActive = i === search.activeIndex;
			const x = match.col_start * m.cellWidth;
			const y = vpRow * m.cellHeight;
			const w = (match.col_end - match.col_start) * m.cellWidth;
			octx.fillStyle = "rgba(255, 180, 50, 0.2)";
			octx.fillRect(x, y, w, m.cellHeight);
			if (isActive) {
				octx.fillStyle = "#e8984c";
				octx.fillRect(x, y + m.cellHeight - 2, w, 2);
			}
		}
	}

	/**
	 * Issue #4: "Search in Block" used to resolve its target block silently —
	 * nothing told the user which one they were searching. Draws a thin accent
	 * bar (the same `#e8984c` used for the active search match below) along the
	 * left edge of the scoped block's row range, clamped to whatever's currently
	 * in the viewport.
	 */
	function paintSearchScopeIndicator(m: CellMetrics) {
		if (!searchBlockScope || !scopedSearchBlock) return;
		const bounds = currentViewportBounds();
		if (!bounds) return;
		// endLine ?? Infinity: an open block's indicator should extend to the bottom
		// of the viewport, same as a closed block would if it ran past it.
		const clamped = clampRowRangeToViewport(
			scopedSearchBlock.promptLine,
			scopedSearchBlock.endLine ?? Number.POSITIVE_INFINITY,
			bounds.viewTop,
			bounds.viewBottom,
		);
		if (!clamped) return; // scrolled fully off-screen — draw nothing, not a full-height bar
		const { startVp, endVp } = clamped;
		octx.fillStyle = "#e8984c";
		octx.fillRect(0, startVp * m.cellHeight, 2, (endVp - startVp + 1) * m.cellHeight);
	}

	/** Exit-status colors for the gutter mark — GitHub-style red/green. `canvasTerminalMarks.ts`'s
	 *  scrollbar ticks use blue for "not a failure" (a different surface with a different
	 *  question: prompt-submission ticks there are their own dedicated green), so this mark's
	 *  green is deliberately its own choice, not a value borrowed from that module. */
	const GUTTER_MARK_COLOR = { success: "#3fb950", failure: "#f85149" } as const;

	function paintGutterMarkers(m: CellMetrics) {
		const term = terminalsStore.get(props.terminalId);
		if (!term) return;
		const blocks = term.commandBlocks;
		if (blocks.length === 0) return;
		for (const block of blocks) {
			const kind = gutterMarkKind(block);
			if (!kind) continue;
			const vpRow = absRowToViewport(block.promptLine);
			if (vpRow === null) continue;
			octx.fillStyle = GUTTER_MARK_COLOR[kind];
			octx.fillRect(-GUTTER_PX, vpRow * m.cellHeight, 3, m.cellHeight);
		}
	}

	/** Fold/unfold chevron on each foldable block's header row (`executionLine ?? promptLine`,
	 *  same row `gutterZoneAt` treats as the fold zone for a gutter click). Drawn for every
	 *  closed block regardless of fold state — issue #6's split-zone gutter click needs a
	 *  visible target, and it doubles as the "is this block folded" indicator that used to be
	 *  a blue bar in `paintFoldedBlocks` easily mistaken for a status mark (issue #7). */
	function paintFoldChevrons(m: CellMetrics) {
		if (!settingsStore.state.blockFoldingEnabled) return;
		const term = terminalsStore.get(props.terminalId);
		if (!term || term.commandBlocks.length === 0) return;
		const fontFamily = settingsStore.getFontFamily();
		octx.font = `${Math.round(m.cellHeight * 0.7)}px ${fontFamily}`;
		octx.fillStyle = "rgba(150,150,150,0.8)";
		for (const block of term.commandBlocks) {
			const folded = term.foldedBlocks.has(block.promptLine);
			if (!folded && !foldRange(block)) continue; // nothing to fold, don't imply otherwise
			const headerRow = block.executionLine ?? block.promptLine;
			const vpRow = absRowToViewport(headerRow);
			if (vpRow === null) continue;
			const y = vpRow * m.cellHeight;
			octx.fillText(folded ? "▸" : "▾", -GUTTER_PX + 4, y + m.cellHeight * 0.8);
		}
	}

	function paintFoldedBlocks(m: CellMetrics) {
		const term = terminalsStore.get(props.terminalId);
		if (!term || term.foldedBlocks.size === 0) return;
		const fontFamily = settingsStore.getFontFamily();
		const painted = new Set<number>();
		for (const promptLine of term.foldedBlocks) {
			if (painted.has(promptLine)) continue;
			painted.add(promptLine);
			const block = term.commandBlocks.find((b) => b.promptLine === promptLine);
			if (!block) continue;
			const range = foldRange(block);
			if (!range) continue;
			const { foldStart, foldedCount } = range;
			const foldEnd = foldStart + foldedCount;
			const bounds = currentViewportBounds();
			const clamped = bounds ? clampRowRangeToViewport(foldStart, foldEnd, bounds.viewTop, bounds.viewBottom) : null;
			if (!clamped) continue;
			const { startVp, endVp } = clamped;
			const y = startVp * m.cellHeight;
			const h = (endVp - startVp + 1) * m.cellHeight;
			// Fully opaque, and reserves the folded rows' original height rather than
			// removing it (true row-remapping collapse is a larger follow-up — see
			// docs/user-guide/terminals.md). Issue #1: this used to be globalAlpha=0.85,
			// which left 15% of the folded text visibly bleeding through — "N lines
			// folded" while still being able to read those N lines. The overlay canvas
			// paints on top of the base canvas (see the JSX: overlay is later in the
			// DOM, same stacking context, no explicit z-index), so opaque here is
			// sufficient to fully hide the text underneath without touching the grid
			// renderer's own paint path.
			octx.fillStyle = cachedBgDefault;
			octx.fillRect(-GUTTER_PX, y, overlayCanvasRef.width / m.dpr, h);
			const label = `  ··· ${foldedCount} lines folded ···`;
			octx.font = `${Math.round(m.cellHeight * 0.7)}px ${fontFamily}`;
			octx.fillStyle = "rgba(150,150,150,0.6)";
			octx.fillText(label, 4, y + m.cellHeight * 0.75);
		}
	}

	let blockTimestampsVisible = false;

	function paintBlockTimestamps(m: CellMetrics) {
		const mode = settingsStore.state.blockTimestampMode;
		if (mode === "off") return;
		if (mode === "modifier" && !blockTimestampsVisible) return;
		const term = terminalsStore.get(props.terminalId);
		if (!term) return;
		const all = term.activeBlock ? [...term.commandBlocks, term.activeBlock] : term.commandBlocks;
		if (all.length === 0) return;
		const fontFamily = settingsStore.getFontFamily();
		const fontSize = Math.round(m.cellHeight * 0.7);
		octx.font = `${fontSize}px ${fontFamily}`;
		octx.fillStyle = "rgba(150,150,150,0.5)";
		const canvasW = overlayCanvasRef.width / m.dpr;
		let lastLabelBottom = -Infinity;
		for (const block of all) {
			const vpRow = absRowToViewport(block.promptLine);
			if (vpRow === null) continue;
			const y = vpRow * m.cellHeight;
			if (y < lastLabelBottom) continue;
			const label = formatRelativeTime(Date.now() - block.startedAt);
			const tw = octx.measureText(label).width;
			octx.fillText(label, canvasW - tw - 8, y + m.cellHeight * 0.75);
			lastLabelBottom = y + m.cellHeight;
		}
	}

	function paintSelection(m: CellMetrics) {
		if (!selection.start || !selection.end) return;
		const absStartRow = Math.min(selection.start.row, selection.end.row);
		const absEndRow = Math.max(selection.start.row, selection.end.row);

		octx.fillStyle = "rgba(58, 130, 220, 0.35)";

		for (let absRi = absStartRow; absRi <= absEndRow; absRi++) {
			const vpRow = absRowToViewport(absRi);
			if (vpRow === null) continue;
			// During a gesture rows come from the cache (keyed by the eviction-stable
			// all-time index = historyBase + grid-relative abs); at rest from the live
			// rowMap (keyed by viewport row). `absRi` is the grid-relative selection
			// coordinate, so bridge it into the cache's space with historyBase.
			const row =
				overlayScrollOffset != null ? rowCache.get((currentFrame?.historyBase ?? 0) + absRi) : rowMap.get(vpRow);
			if (!row) continue;
			const y = vpRow * m.cellHeight;

			if (absStartRow === absEndRow) {
				const c0 = Math.min(selection.start.col, selection.end.col);
				const c1 = Math.max(selection.start.col, selection.end.col);
				octx.fillRect(c0 * m.cellWidth, y, (c1 - c0 + 1) * m.cellWidth, m.cellHeight);
			} else if (absRi === absStartRow) {
				const isStartFirst = selection.start.row <= selection.end.row;
				const startCol = isStartFirst ? selection.start.col : selection.end.col;
				octx.fillRect(startCol * m.cellWidth, y, (row.count - startCol) * m.cellWidth, m.cellHeight);
			} else if (absRi === absEndRow) {
				const isStartFirst = selection.start.row <= selection.end.row;
				const endCol = isStartFirst ? selection.end.col : selection.start.col;
				octx.fillRect(0, y, (endCol + 1) * m.cellWidth, m.cellHeight);
			} else {
				octx.fillRect(0, y, row.count * m.cellWidth, m.cellHeight);
			}
		}
	}

	function getLocalSelectionText(): string {
		return selection.getLocalText((absRi) => {
			const vpRow = absRowToViewport(absRi);
			return vpRow !== null ? (rowMap.get(vpRow) ?? null) : null;
		});
	}

	function paintCursor(frame: DecodedFrame, m: CellMetrics) {
		if (frame.displayOffset > 0) return;
		if (!frame.cursorVisible) return;
		if (!focused()) return;
		if (!shouldPaintCursor(cursorBlinkOn, frame.cursorSteady)) return;

		const settingShape: CursorShape =
			settingsStore.state.cursorStyle === "block"
				? "block"
				: settingsStore.state.cursorStyle === "underline"
					? "underline"
					: "beam";
		const shape: CursorShape = resolveCursorShape(frame.cursorShape, settingShape);

		// Measure the glyph under the cursor up front (not just for "block") so a
		// wide character (CJK, emoji, …) widens a block/underline cursor to cover
		// both columns it occupies — computeCursorRect ignores spanCols for beam.
		const row = rowMap.get(frame.cursorRow);
		const col = frame.cursorCol;
		let cp = 0;
		let spanCols: 1 | 2 = 1;
		if (row && col < row.count) {
			cp = row.codepoints[col];
			if (cp !== 0 && cp !== 0x20) {
				const fontFamily = settingsStore.getFontFamily();
				octx.font = gridRenderer.buildFontStyle(row.attrs[col], m.fontSize, fontFamily);
				if (isWideCursorGlyph(octx.measureText(String.fromCodePoint(cp)).width, m.cellWidth)) {
					spanCols = 2;
				}
			}
		}

		const rect = computeCursorRect(shape, frame.cursorRow, frame.cursorCol, m, spanCols);

		octx.fillStyle = cachedFgDefault;
		octx.fillRect(rect.x, rect.y, rect.w, rect.h);

		if (shape === "block" && cp !== 0 && cp !== 0x20) {
			octx.fillStyle = cachedBgDefault;
			octx.fillText(String.fromCodePoint(cp), rect.x, frame.cursorRow * m.cellHeight + m.baseline);
		}

		syncImePosition(frame.cursorRow, frame.cursorCol, m);
	}

	function syncImePosition(row: number, col: number, m: CellMetrics) {
		const x = GUTTER_PX + col * m.cellWidth;
		const y = row * m.cellHeight;
		keyInputRef.style.left = `${x}px`;
		keyInputRef.style.top = `${y}px`;
		keyInputRef.style.height = `${m.cellHeight}px`;
		keyInputRef.style.fontSize = `${m.fontSize}px`;
	}

	// --- Scrollbar ---

	function updateScrollbar(frame: DecodedFrame) {
		if (!scrollbarRef || !scrollThumbRef) return;
		const total = frame.historySize + (frame.screenRows || lastResizeRows || 24);
		// visible rows = the authoritative resize row count — no per-frame
		// canvasRef.clientHeight read (layout-forcing).
		const visible = lastResizeRows || 24;

		const term = terminalsStore.get(props.terminalId);
		const showScrollbar = shouldShowScrollbar({
			historySize: frame.historySize,
			showBlockMarks: settingsStore.state.showBlockMarks,
			showPromptMarks: settingsStore.state.showPromptMarks,
			blocks: term?.commandBlocks ?? [],
			promptLines: term?.userPromptLines ?? [],
		});
		if (!showScrollbar) {
			scrollbarRef.style.display = "none";
			return;
		}
		scrollbarRef.style.display = "block";

		if (frame.historySize === 0) {
			// Nothing to scroll, but there are marks to show: collapse the thumb to
			// fill the track (no drag affordance) rather than sizing it as if there
			// were scrollable content.
			scrollThumbRef.style.height = "100%";
			scrollThumbRef.style.transform = "translateY(0px)";
		} else {
			// Track height comes from the resize-time cache, not scrollbarRef.clientHeight.
			const trackH = scrollbarTrackHeight;
			const thumbRatio = Math.min(1, visible / total);
			const thumbHeight = Math.max(20, trackH * thumbRatio);
			const scrollRange = trackH - thumbHeight;
			const scrollPos = (1 - frame.displayOffset / frame.historySize) * scrollRange;

			scrollThumbRef.style.height = `${thumbHeight}px`;
			scrollThumbRef.style.transform = `translateY(${scrollPos}px)`;
		}

		paintScrollbarMarks(total);
	}

	let scrollbarMarksContainer: HTMLDivElement | null = null;
	let lastScrollbarMarksKey = "";

	function paintScrollbarMarks(totalRows: number) {
		if (!scrollbarRef) return;
		const term = terminalsStore.get(props.terminalId);
		if (!term) return;

		const marksInput: ScrollbarMarksInput = {
			showBlockMarks: settingsStore.state.showBlockMarks,
			showPromptMarks: settingsStore.state.showPromptMarks,
			blocks: term.commandBlocks,
			promptLines: term.userPromptLines,
			totalRows,
			matches: search.matches,
		};

		const key = scrollbarMarksKey(marksInput);
		if (key === lastScrollbarMarksKey) return;
		lastScrollbarMarksKey = key;

		if (!scrollbarMarksContainer) {
			scrollbarMarksContainer = document.createElement("div");
			scrollbarMarksContainer.style.cssText =
				"position:absolute;top:0;left:0;width:100%;height:100%;pointer-events:none";
			scrollbarRef.appendChild(scrollbarMarksContainer);
		}

		scrollbarMarksContainer.innerHTML = scrollbarMarksHtml(marksInput, scrollbarTrackHeight);
	}

	// --- Suggest / Intent overlay ---

	const rowToText = rowText;

	function makeOverlayDiv(top: number, height: number, background: string): HTMLDivElement {
		const div = document.createElement("div");
		div.style.cssText = `position:absolute;left:0;right:0;top:${top}px;height:${height}px;background:${background}`;
		return div;
	}

	// Cached suggest/intent overlay state to avoid full DOM rebuild
	let lastSuggestOverlayKey = "";

	function updateSuggestOverlay(
		_frame: DecodedFrame,
		m: CellMetrics,
		dirtyIndices?: Set<number>,
		snapshotOverride?: (i: number) => { text: string; isWrapped: boolean } | null,
	) {
		if (!overlayRef) return;

		// Skip full rescan if no dirty rows touch suggest/intent patterns
		// (skipped entirely when rendering from the cache during a scroll gesture).
		if (!snapshotOverride && dirtyIndices && !fullRepaintNeeded) {
			let hasSuggestContent = false;
			for (const idx of dirtyIndices) {
				const row = rowMap.get(idx);
				if (!row) continue;
				const text = rowToText(row);
				if (SUGGEST_ANCHOR_RE.test(text) || INTENT_HIGHLIGHT_RE.test(text)) {
					hasSuggestContent = true;
					break;
				}
			}
			if (!hasSuggestContent) {
				if (lastSuggestOverlayKey === "") return;
				// Stale overlay — fall through to rebuild/clear it
			}
		}

		const bg = cachedBgDefault;
		const numRows = lastResizeRows || 24;

		const getRowSnapshot =
			snapshotOverride ??
			((i: number) => {
				const row = rowMap.get(i);
				if (!row) return null;
				return { text: rowToText(row), isWrapped: row.wrapped };
			});

		// Decide what to mask before building anything: most repaints leave the
		// plan untouched, and the elements were being created and dropped just to
		// discover that.
		const { key, blocks } = planSuggestOverlay(numRows, getRowSnapshot);
		if (key === lastSuggestOverlayKey) return;
		lastSuggestOverlayKey = key;

		overlayRef.textContent = "";
		for (const block of blocks) {
			const background = block.kind === "intent" ? "rgba(181,147,90,0.12)" : bg;
			overlayRef.appendChild(makeOverlayDiv(block.row * m.cellHeight, m.cellHeight, background));
		}
	}

	function startBlink() {
		if (blinkInterval != null) return;
		cursorBlinkOn = true;
		blinkResetAt = performance.now();
		blinkInterval = setInterval(() => {
			const elapsed = performance.now() - blinkResetAt;
			const phase = Math.floor(elapsed / 700) % 2 === 0;
			if (cursorBlinkOn !== phase) {
				cursorBlinkOn = phase;
				if (rafId === undefined) {
					rafId = requestAnimationFrame(() => {
						rafId = undefined;
						if (!alive || hidden) return;
						const m = metrics();
						if (currentFrame && m) repaintCursorOnly(currentFrame, m);
					});
				}
			}
		}, 700);
	}

	function stopBlink() {
		if (blinkInterval != null) {
			clearInterval(blinkInterval);
			blinkInterval = undefined;
		}
	}

	function resetBlink() {
		cursorBlinkOn = true;
		blinkResetAt = performance.now();
		if (blinkInterval == null) startBlink();
	}

	function repaintCursorOnly(frame: DecodedFrame, m: CellMetrics) {
		repaintOverlay(frame, m);
	}

	function repaintCursorIfNeeded() {
		const m = metrics();
		if (currentFrame && m) repaintCursorOnly(currentFrame, m);
	}

	// Coalesced scroll: handlers compute the next absolute display offset
	// (latest-wins) and a single rAF flush sends it to the backend, with at most
	// one IPC in flight. Decouples input rate from IPC and avoids delta desync.
	let scrollRafId = 0;

	function scheduleScrollFlush() {
		if (!scrollRafId) scrollRafId = requestAnimationFrame(flushScroll);
	}
	function flushScroll() {
		scrollRafId = 0;
		if (scroll.pendingOffset == null) return;
		if (scroll.inFlight) {
			scheduleScrollFlush(); // retry next frame, keep pending
			return;
		}
		const target = scroll.pendingOffset;
		scroll.pendingOffset = null;
		scroll.inFlight = true;
		invokeRef?.("terminal_scroll_to_offset", { sessionId: props.sessionId, offset: target })
			.catch(ipcErr("terminal_scroll_to_offset"))
			.finally(() => {
				scroll.inFlight = false;
			});
	}

	// --- Smooth (sub-line) scroll, main renderer only ---
	// `scrollPosF` is the desired fractional display offset (in lines). The
	// integer part is committed to the backend (above); the fractional remainder
	// is shown as a transient translateY of the stage, with the adjacent overscan
	// row sliding into view. On gesture end it animates to the nearest line.
	// At rest (scrollPosF === null) the transform is identity → geometry unchanged.
	let smoothRafId = 0;
	let overlaysHiddenForScroll = false;
	// When non-null, the overlay (selection/cursor/search) is painted against this
	// integer display offset + the row cache instead of the live backend frame, so
	// it stays aligned with the cache-rendered base during a smooth-scroll gesture.
	let overlayScrollOffset: number | null = null;
	// True only while a gesture is actively producing deltas; false at rest (incl.
	// a fractional rest where scrollPosF stays non-integer — we never snap to a line).
	// We only ever hand off to normal rendering at the bottom (offset 0). Until the
	// backend frame reaches `settlePending` we keep cache-rendering it (no jump).
	let settleTimer = 0;

	// Full-frame reconciliation: partial frames (only the rows alacritty marked
	// dirty) merge into rowMap by index, so if the grid shifts content the canvas
	// can strand stale rows (duplicate/triplicate or vanished blocks) while the grid
	// itself stays correct. After an output burst settles, request one full frame so
	// the next onFrame does a fullReplace and rebuilds rowMap from the grid — a
	// self-heal that can't drift. Gated to at-rest, following-output (offset 0).
	let reconcileTimer: ReturnType<typeof setTimeout> | undefined;
	// First schedule of the current reschedule burst — bounds the total deferral
	// (reconcileDelay caps the trailing debounce at RECONCILE_MAX_WAIT_MS, so
	// sustained partial-frame output can't starve the self-heal forever).
	let reconcileBurstStart: number | null = null;
	// [dup] desync detector (perfDebug only): set when scheduleReconcile fires a
	// forced full-frame request, consumed by the next full frame in onFrame to diff
	// the partial-accumulated rowMap against the authoritative grid. See onFrame.
	let reconcileHealPending = false;
	function scheduleReconcile() {
		const now = Date.now();
		if (reconcileBurstStart == null) reconcileBurstStart = now;
		if (reconcileTimer) clearTimeout(reconcileTimer);
		reconcileTimer = setTimeout(
			() => {
				reconcileTimer = undefined;
				reconcileBurstStart = null;
				const off = currentFrame?.displayOffset ?? -1;
				const fire = shouldFireReconcile({
					alive,
					hidden,
					isScrolling: scroll.scrolling,
					scrollPosF: scroll.position,
					displayOffset: off,
				});
				if (!fire) {
					return;
				}
				// [dup] arm the desync detector: the frame this request pulls back is a
				// full grid snapshot at rest, so the next full frame's diff against the
				// partial-accumulated rowMap reveals any stranded stale rows.
				if (isPerfDebug()) reconcileHealPending = true;
				invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
			},
			reconcileDelay(now, reconcileBurstStart),
		);
	}

	function finishSettle() {
		settleTimer = 0;
		// A settle timer can fire after unmount; the row cache is released by then,
		// so endSmoothScroll must not run.
		if (!alive || scroll.settleTarget == null) return;
		scroll.settleTarget = null;
		scroll.position = null;
		endSmoothScroll();
	}
	function clearSettlePending() {
		if (settleTimer) {
			clearTimeout(settleTimer);
			settleTimer = 0;
		}
		scroll.settleTarget = null;
	}

	function scheduleSmoothRender() {
		if (!smoothRafId)
			smoothRafId = requestAnimationFrame(() => {
				smoothRafId = 0;
				renderSmooth();
			});
	}

	// Repaint the base canvas + the partial rows above/below locally from the row
	// cache for integer display offset `intOffset` — no backend round-trip, so it
	// keeps up at 60fps regardless of scroll speed.
	function renderCachedBase(intOffset: number, m: CellMetrics, rows: number, hist: number) {
		const cacheRow = (abs: number): DecodedRow | null => rowCache.get(abs) ?? null;
		const tempMap = new Map<number, DecodedRow>();
		for (let r = 0; r < rows; r++) {
			const cached = cacheRow(hist - intOffset + r);
			if (cached) tempMap.set(r, cached.index === r ? cached : { ...cached, index: r });
		}
		gridRenderer.paintGrid(tempMap, m, { fullRepaint: true });
		if (octxOverscan && overscanRenderer) {
			const ch = m.cellHeight;
			const w = overscanCanvasRef.width / m.dpr;
			octxOverscan.clearRect(-GUTTER_PX, 0, w, overscanCanvasRef.height / m.dpr);
			const above = cacheRow(hist - intOffset - 1);
			const below = cacheRow(hist - intOffset + rows);
			if (above) {
				octxOverscan.fillStyle = cachedBgDefault;
				octxOverscan.fillRect(-GUTTER_PX, 0, w, ch);
				overscanRenderer.paintRow(above, 0, m);
			}
			if (below) {
				const y = (rows + 1) * ch;
				octxOverscan.fillStyle = cachedBgDefault;
				octxOverscan.fillRect(-GUTTER_PX, y, w, ch);
				overscanRenderer.paintRow(below, y, m);
			}
		}
		ensureCacheBand(intOffset, rows, hist);
		// Rebuild suggest/intent masks from the cache at this offset so they track the
		// scrolling content instead of the lagging backend frame (no flicker, and the
		// raw suggest line stays masked).
		if (currentFrame) {
			updateSuggestOverlay(currentFrame, m, undefined, (i) => {
				const cached = cacheRow(hist - intOffset + i);
				if (!cached) return null;
				return { text: rowToText(cached), isWrapped: cached.wrapped };
			});
		}
	}

	function renderSmooth() {
		// A queued smooth-render RAF can fire after unmount; the row cache it reads
		// is released by then, so bail before touching it.
		if (!alive || scroll.position == null || !currentFrame) return;
		const m = metrics();
		if (!m) return;
		const ch = m.cellHeight;
		const rows = lastResizeRows || 24;
		// All-time top-of-history index: cache keys live in this eviction-stable space.
		const hist = currentFrame.historyBase + currentFrame.historySize;
		const intOffset = Math.floor(scroll.position);
		const frac = (scroll.position - intOffset) * ch; // [0, ch): how far past the line
		renderCachedBase(intOffset, m, rows, hist);
		// Repaint the selection/cursor/search overlay aligned to the cached offset so the
		// highlight tracks the content while scrolling (the overlay canvas is inside the
		// stage, so the fractional translate below keeps it pixel-aligned with the base).
		overlayScrollOffset = intOffset;
		repaintOverlay(currentFrame, m);
		overlayScrollOffset = null;
		stageRef.style.transform = `translate3d(0, ${frac}px, 0)`;
		// Track the scrollbar thumb live against the fractional position (paintFrame,
		// which normally drives it, is suppressed during the gesture).
		updateScrollbar({ ...currentFrame, displayOffset: scroll.position });
	}

	// Background-fetch any missing 64-row chunks in a one-screen band around the
	// viewport so fast scrolling always has cached rows ready to paint.
	function ensureCacheBand(intOffset: number, rows: number, hist: number) {
		if (!invokeRef) return;
		const lo = Math.max(0, hist - intOffset - rows);
		const hi = hist - intOffset + 2 * rows;
		const firstChunk = Math.floor(lo / ROW_CACHE_CHUNK);
		const lastChunk = Math.floor(hi / ROW_CACHE_CHUNK);
		for (let chunk = firstChunk; chunk <= lastChunk; chunk++) {
			if (chunk < 0 || requestedChunks.has(chunk)) continue;
			requestedChunks.add(chunk);
			void fetchChunk(chunk);
		}
	}

	async function fetchChunk(chunk: number) {
		if (!invokeRef) return;
		const start = chunk * ROW_CACHE_CHUNK;
		const cacheGeneration = scroll.cacheGeneration;
		try {
			const res = await invokeRef("terminal_styled_rows", {
				sessionId: props.sessionId,
				start,
				count: ROW_CACHE_CHUNK,
			});
			// Unmounted during the await: the row cache is released, so don't
			// repopulate it or schedule a render against it.
			if (!alive || !scroll.isCacheGenerationCurrent(cacheGeneration)) return;
			// Both transports deliver raw bytes; the shape still gets checked so a
			// non-binary answer is dropped rather than decoded as an empty chunk.
			const buffer = toBinaryPayload(res);
			if (!buffer) return;
			const decoded = decodeStyledRange(buffer);
			if (!decoded) return;
			scroll.cacheRows(decoded.rows);
			if (scroll.position != null) scheduleSmoothRender();
		} catch (e) {
			if (!scroll.isCacheGenerationCurrent(cacheGeneration)) return;
			requestedChunks.delete(chunk);
			ipcErr("terminal_styled_rows")(e);
		}
	}

	// During a gesture the cursor/selection canvas is hidden (those are anchored to
	// the backend frame and we're not selecting while scrolling). The suggest/intent
	// masks (overlayRef) stay visible — they're rebuilt from the cache and scroll
	// with the content, so they neither flicker nor uncover the raw suggest text.
	function setScrollOverlaysHidden(hidden: boolean) {
		if (overlaysHiddenForScroll === hidden) return;
		overlaysHiddenForScroll = hidden;
		if (overlayCanvasRef) overlayCanvasRef.style.visibility = hidden ? "hidden" : "";
	}

	// Wipe the overscan canvas. The above/below rows are only meaningful mid-gesture
	// while the stage slides; at rest the (opaque) base canvas covers the viewport but
	// the overscan's below-row strip peeks out beneath it. Leaving the last gesture's
	// row there shows it as a ghost line below the viewport, so clear on every return
	// to rest.
	function clearOverscan() {
		if (!octxOverscan || !overscanCanvasRef) return;
		const dpr = metrics()?.dpr ?? window.devicePixelRatio ?? 1;
		octxOverscan.clearRect(-GUTTER_PX, 0, overscanCanvasRef.width / dpr, overscanCanvasRef.height / dpr);
	}

	// Leave smooth-scroll mode: restore the overlays and repaint the base from the
	// real backend frame at its committed offset.
	function endSmoothScroll() {
		setScrollOverlaysHidden(false);
		if (stageRef) stageRef.style.transform = "";
		clearOverscan();
		const m = metrics();
		if (currentFrame && m) {
			fullRepaintNeeded = true;
			paintFrame(currentFrame, m);
		}
	}

	// Cancel an in-flight smooth gesture and restore the resting state. Self-contained:
	// also cancels the wheel gesture-end timer so a late resetScrollGesture can't fire
	// after we've handed control to another scroll path (scrollbar, programmatic jump).
	function resetSmoothScroll(repaint = true) {
		clearTimeout(scrollGestureEndTimer);
		if (scrollRafId) {
			cancelAnimationFrame(scrollRafId);
			scrollRafId = 0;
		}
		if (smoothRafId) {
			cancelAnimationFrame(smoothRafId);
			smoothRafId = 0;
		}
		const hadSmoothPosition = scroll.position != null;
		clearSettlePending();
		scroll.cancel();
		if (hadSmoothPosition && repaint) {
			endSmoothScroll();
		} else if (!repaint) {
			setScrollOverlaysHidden(false);
			if (stageRef) stageRef.style.transform = "";
			clearOverscan();
		}
	}

	// Seed the cache with the current viewport's rows so the first frame of a gesture
	// has content to paint immediately (the band prefetch fills the rest).
	function seedCacheFromCurrentFrame() {
		if (!currentFrame) return;
		const base = currentFrame.historyBase + currentFrame.historySize - currentFrame.displayOffset;
		scroll.cacheRows(Array.from(rowMap, ([r, row]) => ({ abs: base + r, row })));
	}

	function applySmoothScroll(deltaLines: number) {
		if (scroll.position == null) {
			// Entering a gesture: rebuild the cache from the current era (drops rows
			// staled by scrollback eviction). The overlay is NOT hidden — renderSmooth
			// repaints it from the cache so the selection highlight survives the scroll.
			scroll.clearCache();
			seedCacheFromCurrentFrame();
		}
		clearSettlePending();
		const hist = currentFrame?.historySize ?? 0;
		const position = scroll.applyDelta(deltaLines, currentFrame?.displayOffset ?? 0, hist);
		// Commit the integer floor so the backend display tracks the cache base.
		scheduleScrollFlush();
		// Reached the bottom — hand off to normal rendering (resume following output)
		// once the backend frame arrives at offset 0. No motion: 0 has no fractional part.
		if (position === 0) {
			scroll.settleTarget = 0;
			if (settleTimer) clearTimeout(settleTimer);
			settleTimer = window.setTimeout(finishSettle, 400);
		}
		scheduleSmoothRender();
	}

	// Gesture ended. Snap to the nearest line and hand off to normal (backend-frame)
	// rendering so the resting state always has scrollPosF === null. The old no-snap
	// behavior left scrollPosF at a fractional rest indefinitely; scheduleRepaint()
	// bails while scrollPosF != null, so the base canvas would never repaint again →
	// BLACK on the next resize / repo-switch / split-move (only on terminals that had
	// been scrolled, hence the history-size correlation). Snapping settles scrollPosF
	// back to null so repaints resume by construction.
	function resetScrollGesture() {
		const target = scroll.snap();
		if (target == null) return;
		scheduleScrollFlush();
		if (currentFrame && currentFrame.displayOffset === target) {
			// Backend already sits on the snapped line — no new frame will arrive to
			// trigger the onFrame handoff, so commit to normal rendering right now.
			clearSettlePending();
			scroll.position = null;
			endSmoothScroll();
			return;
		}
		// Otherwise keep cache-rendering the snapped line until the backend frame
		// reaches `target` (onFrame settle handler), with a timer as the safety net.
		scroll.settleTarget = target;
		if (settleTimer) clearTimeout(settleTimer);
		settleTimer = window.setTimeout(finishSettle, 400);
		scheduleSmoothRender();
	}

	// Forwarded-wheel gesture went idle: drop the pixel accumulator (a trailing
	// sub-notch residual must not leak into the next, unrelated gesture) and let any
	// in-flight scrollback gesture settle the normal way. resetScrollGesture is a
	// no-op when there is no smooth-scroll position, so this is safe to call even when
	// the wheel was being forwarded to the app the whole time.
	function onWheelGestureEnd() {
		resetWheelNotch(wheelNotch);
		resetScrollGesture();
	}

	// Apply one wheel/touch delta (raw pixels) with gesture acceleration → smooth
	// sub-line scroll. The acceleration ramp is capped (gestureAccelFactor) and resets
	// on direction reversal (accumulateGesture) so a long momentum fling can't ramp
	// past a 2x ceiling or keep accelerating a scroll-back after the gesture reverses.
	function handleScrollDelta(dy: number) {
		const m = metrics();
		const ch = m?.cellHeight ?? 20;
		const screenPx = ch * (lastResizeRows || 24);
		const distance = scroll.accumulateGesture(dy);
		applySmoothScroll((dy * gestureAccelFactor(distance, screenPx)) / ch);
	}

	// The transport normalizes every payload shape (see toBinaryPayload) and drops
	// the ones that are not binary, so a frame arrives here as bytes or not at all.
	function onFrame(buffer: ArrayBuffer) {
		// Freeze-investigation: a frame storm starving the rAF loop breadcrumbs here.
		markPerf("term.onFrame");

		// Frame receipt ordering: ack FIRST (the ack only reopens the delivery gate;
		// the ticker sends the next frame on its own schedule, so ack must never wait
		// on decode/paint), then decode (cheap) which keeps rowMap + currentFrame alive
		// for the overlay (cursor/selection/links/search/scrollbar) and input semantics.
		//
		// A hidden terminal acks late instead of per frame. That is the whole of the
		// flow control this observer promises: a background tab is display:none and
		// never unmounted, so Rust keeps a producer running for it, and acking at
		// once reopens the gate at full rate for a frame nobody can see. The trailing
		// ack drops it to ~2 frames/s while still clearing the gate itself — staying
		// silent would leave that to the ticker's stuck-frontend path, which is a
		// warning per output burst for a tab that is merely in the background.
		framesReceived++;
		if (hidden) hiddenAck.schedule();
		else ackFrame();
		const timing = isFrameTimingEnabled();
		const decodeT0 = timing ? performance.now() : 0;
		// rowMap is the merge base for partial rows (see ROW_PARTIAL_FLAG): they
		// carry only their damaged columns and take the rest of the line from what
		// is already on screen.
		const frame = decodeBinaryFrame(buffer, rowMap);
		if (timing) recordFrameTiming(props.sessionId, "decode", performance.now() - decodeT0);
		if (!frame) {
			// The only way to land here is a buffer shorter than the 26-byte header:
			// truncated row data decodes into the rows that did survive. The backend
			// never sends one, so this is a wire-format bug and must not be silent.
			// DEFERRED (2026-08-18) — no resync request on this path. The rows of a
			// dropped frame are gone (damage was consumed backend-side), but asking
			// for a full frame on a stream that is producing malformed buffers turns
			// one bad frame into a request loop at IPC rate. Revisit if this log is
			// ever seen in the wild — then we know the real failure mode.
			appLogger.error("terminal", "grid frame too short to decode", {
				sessionId: props.sessionId,
				byteLength: buffer.byteLength,
			});
			return;
		}

		// Decoded even while hidden: the bell rides in the frame header and a
		// background tab is exactly where it needs to reach the user.
		if (frame.bell) props.onBell?.();
		// Everything below paints, scans links or fills the scroll cache for a
		// viewport that is not on screen.
		if (hidden) return;

		// Grid decision: geom/scroll/full-replace/scroll-wait for the rowMap.
		const decision = decideFrameGrid(
			{ lastScreenRows, lastScreenCols, lastDisplayOffset, lastHistorySize, lastAltScreen },
			frame,
			lastResizeRows,
		);
		const { geomChanged, scrollChanged, screenChanged } = decision;

		// A primary/alternate swap replaces the entire absolute-row universe. Reset
		// every stateful consumer as one transaction, and never repaint the old smooth
		// frame while adopting the new one.
		if (screenChanged) {
			screenGeneration++;
			resetSmoothScroll(false);
			scroll.clearCache();
			resetWheelNotch(wheelNotch);
			selection.clear();
			stopSelectionScroll();
			clearSearchState();
			rowMap.clear();
			pendingDirtyRows.clear();
			clearDetectedLinks();
			linkMenu.close();
			hoveredLink = null;
			canvasRef.style.cursor = "text";
			if (reconcileTimer) clearTimeout(reconcileTimer);
			reconcileTimer = undefined;
			reconcileBurstStart = null;
			reconcileHealPending = false;
			fullRepaintNeeded = true;
		}

		// When geometry changes, viewport is entirely different — must clear and repaint
		if (geomChanged) {
			selection.clear();
			rowMap.clear();
			clearDetectedLinks();
			fullRepaintNeeded = true;
		}

		if (scrollChanged || geomChanged || screenChanged) {
			lastDisplayOffset = frame.displayOffset;
			lastHistorySize = frame.historySize;
			lastScreenRows = frame.screenRows;
			lastScreenCols = frame.screenCols;
			lastAltScreen = frame.altScreen;
			if (hoveredLink) {
				hoveredLink = null;
				canvasRef.style.cursor = "text";
			}
		}

		// [dup] desync detector (perfDebug): if this full frame is the one pulled by a
		// reconcile at rest (no scroll/geom change), snapshot the partial-accumulated
		// rowMap BEFORE it's replaced. Any viewport row that differs from the grid below
		// was stale on the canvas — a stranded row the partial frames never carried
		// (alacritty damage under-report on in-place TUI redraws). Logged after the merge.
		let dupHealSnapshot: Map<number, string> | null = null;
		if (reconcileHealPending && decision.fullReplace && !decision.geomChanged && !decision.scrollChanged) {
			dupHealSnapshot = new Map();
			for (const [idx, row] of rowMap) dupHealSnapshot.set(idx, rowToText(row));
		}

		// When backend sends all screen rows, replace rowMap to discard stale entries
		if (decision.fullReplace) {
			reconcileHealPending = false;
			rowMap.clear();
			clearDetectedLinks();

			fullRepaintNeeded = true;
		} else if (decision.scrollWait) {
			// Scroll changed but only partial rows arrived. Old rowMap entries are keyed
			// to the previous viewportTop — rendering them with the new displayOffset maps
			// them to wrong screen positions, producing ghost content.
			// Clear immediately (brief blank < ~5ms) and request a full frame.
			rowMap.clear();
			clearDetectedLinks();
			fullRepaintNeeded = true;
			invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
			currentFrame = frame;
			return;
		}
		for (const row of frame.rows) {
			rowMap.set(row.index, row);
			pendingDirtyRows.add(row.index);
			scanRowForLinks(row.index);
		}

		// A partial row arrived for a line we do not hold — it was dropped rather
		// than painted with holes, so pull the whole screen back. Self-limiting:
		// terminal_request_frame forces full damage, and a full frame repopulates
		// every row, so the next frame cannot land here again for the same reason.
		if (frame.needsFullFrame) {
			fullRepaintNeeded = true;
			invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
		}

		// Every row in this frame just had its text replaced. A search match anchored
		// to one of those absolute rows describes text that no longer exists, so drop
		// it now rather than painting a highlight over whatever the TUI wrote there
		// (ink agents repaint their bottom rows continuously). The debounced re-search
		// then re-establishes the matches that still hit.
		if (searchQuery && frame.rows.length > 0) {
			const viewportTop = frame.historySize - frame.displayOffset;
			const rewritten = new Set<number>();
			for (const row of frame.rows) rewritten.add(viewportTop + row.index);
			// No repaint flag needed: repaintOverlay() clears and redraws the overlay
			// canvas (where highlights live) on every frame.
			search.dropRows(rewritten);
			scheduleSearchRefresh();
		}

		// [dup] desync detector: diff the pre-heal snapshot against the now-authoritative
		// rowMap. Diverging rows were stale on the canvas until this reconcile healed them.
		if (dupHealSnapshot) {
			const diverged: number[] = [];
			for (const [idx, oldText] of dupHealSnapshot) {
				const nr = rowMap.get(idx);
				const newText = nr ? rowToText(nr) : "";
				if (oldText !== newText) diverged.push(idx);
			}
			if (diverged.length > 0) {
				const sample = diverged.slice(0, 5).map((i) => {
					const nr = rowMap.get(i);
					return `#${i}: "${(dupHealSnapshot?.get(i) ?? "").slice(0, 40)}" => "${(nr ? rowToText(nr) : "").slice(0, 40)}"`;
				});
				appLogger.warn("terminal", `[dup] canvas desync healed: ${diverged.length} stale viewport row(s)`, {
					sessionId: props.sessionId,
					divergedRows: diverged,
					sample,
				});
			}
		}

		currentFrame = frame;

		// Re-anchor the soft-keyboard lift to the new cursor row (touch + keyboard
		// open only; a cheap no-op otherwise). currentFrame is a plain ref, so the
		// keyboard effect can't track cursor moves — this is the cursor-move trigger.
		updateKeyboardLift();

		// Partial frames merge by index and can strand stale rows (grid stays correct,
		// canvas drifts → duplicate/vanished blocks). A full frame already rebuilt the
		// rowMap, so only reconcile after partial frames; the debounce coalesces bursts.
		if (!decision.fullReplace) scheduleReconcile();

		// Smooth scroll: seed the client-side row cache from each frame's rows, keyed
		// by the eviction-stable absolute index `historyBase + historySize -
		// displayOffset + index`. `historyBase` (lines evicted from the history top)
		// climbs by exactly what the grid-relative coordinate loses on eviction, so a
		// physical line keeps its key for life — no stale row aliases onto a new one
		// after the scrollback cap rotates. Also pump a live render if a gesture is active.
		//
		// During a fast gesture the backend frame trails the live scroll position by
		// several lines, so its rows are keyed to a lagging displayOffset. Seeding them
		// then would overwrite cache entries the smooth renderer is currently painting
		// (brief flicker / wrong overscan). Only seed when at rest or when the backend
		// has caught up to our integer offset.
		if (scroll.position == null || frame.displayOffset === Math.floor(scroll.position)) {
			const base = frame.historyBase + frame.historySize - frame.displayOffset;
			scroll.cacheRows(frame.rows.map((row) => ({ abs: base + row.index, row })));
		}
		if (scroll.acceptSettledFrame(frame.displayOffset)) {
			// Backend reached the snapped line — hand off to normal rendering
			// seamlessly (the cache render already shows this exact frame).
			clearSettlePending();
			endSmoothScroll();
		} else if (scroll.scrolling) {
			// Active gesture: re-render against the freshly seeded cache.
			scheduleSmoothRender();
		} else if (scroll.position == null && stageRef?.style.transform) {
			// At rest on a line: clear any stray transform. (Gesture end always
			// snaps to a line and settles scrollPosF → null, so there is no
			// lingering fractional rest to keep a transform alive.)
			stageRef.style.transform = "";
			clearOverscan();
		}

		// Only compare content when the selection is fully on-screen — off-screen rows return empty
		// strings from getLocalSelectionText() causing spurious mismatches that clear the selection.
		if (
			selection.start &&
			selection.cachedText &&
			decision.fullReplace &&
			!selection.spansOffscreen(absRowToViewport)
		) {
			const nowText = getLocalSelectionText();
			if (nowText !== selection.cachedText) selection.clear();
		}

		scheduleRepaint();
		scheduleFileLinkVerification();
	}

	// --- Link detection ---

	const FILE_PATH_RE = filePathRegex();
	const FILE_URL_RE = fileUrlRegex();

	// Per-session cache: row text → verified file link spans (null = checked, none exist)
	const fileLinkCache = linkController.fileCache;
	const FILE_LINK_RECHECK_MS = 3_000;
	const FILE_LINK_CACHE_MAX = 500;

	function scanRowForLinks(rowIndex: number) {
		const row = rowMap.get(rowIndex);
		if (!row) {
			detectedLinks.delete(rowIndex);
			return;
		}
		const text = rowToText(row);
		const spans: { colStart: number; colEnd: number }[] = [];

		for (const url of matchWebUrls(text)) {
			spans.push({ colStart: url.index, colEnd: url.index + url.text.length });
		}

		// File paths: only underline if previously verified to exist
		const cached = fileLinkCache.get(text);
		if (cached?.spans) {
			spans.push(...cached.spans);
		}

		// Spans from links that span soft-wrapped rows (web + file://) — invisible
		// from this single row's text, so merged from the wrapped-link map that
		// verifyVisibleFileLinks() builds via terminal_get_logical_line.
		const wrapped = wrappedLinkSpans.get(rowIndex);
		if (wrapped) spans.push(...wrapped);

		if (spans.length > 0) detectedLinks.set(rowIndex, spans);
		else detectedLinks.delete(rowIndex);
	}

	function scheduleFileLinkVerification() {
		linkController.scheduleVerification(verifyVisibleFileLinks);
	}

	async function verifyVisibleFileLinks() {
		const ref = invokeRef;
		if (!ref || !alive) return;
		const generation = screenGeneration;
		const maxRow = currentFrame?.screenRows || lastResizeRows;
		const cols = lastScreenCols > 0 ? lastScreenCols : currentFrame?.screenCols || 80;
		const now = Date.now();
		const toCheck: { text: string; candidates: { colStart: number; colEnd: number; raw: string }[] }[] = [];

		for (let i = 0; i < maxRow; i++) {
			const row = rowMap.get(i);
			if (!row) continue;
			const text = rowToText(row);

			const cached = fileLinkCache.get(text);
			if (cached) {
				if (cached.spans !== null) continue;
				if (now - cached.ts < FILE_LINK_RECHECK_MS) continue;
			}

			const candidates: { colStart: number; colEnd: number; raw: string }[] = [];
			FILE_PATH_RE.lastIndex = 0;
			let m: RegExpExecArray | null;
			while ((m = FILE_PATH_RE.exec(text)) !== null) {
				const idx = text.indexOf(m[1], m.index);
				candidates.push({ colStart: idx, colEnd: idx + m[1].length, raw: m[1] });
			}
			FILE_URL_RE.lastIndex = 0;
			while ((m = FILE_URL_RE.exec(text)) !== null) {
				candidates.push({ colStart: m.index, colEnd: m.index + m[0].length, raw: m[1] });
			}
			if (candidates.length > 0) toCheck.push({ text, candidates });
		}

		const termId = terminalsStore.getTerminalForSession(props.sessionId);
		const termData = termId ? terminalsStore.get(termId) : undefined;
		const cwd = termData?.cwd || "";
		let anyFound = false;

		// Single-row verification
		// One call for the whole screen. This used to be one IPC per candidate,
		// awaited row by row, so a screen with links on twenty rows cost twenty
		// serial round trips — each one a filesystem-resolving hop for a single
		// string. The backend answers positionally, so the flat result is sliced
		// back onto the rows it came from.
		if (toCheck.length > 0) {
			const flat = toCheck.flatMap((item) => item.candidates.map((c) => c.raw));
			let resolved: (unknown | null)[];
			try {
				resolved = (await ref("resolve_terminal_paths", { cwd, candidates: flat })) as (unknown | null)[];
			} catch (e) {
				appLogger.debug("terminal", "resolve_terminal_paths failed", { count: flat.length, error: e });
				resolved = [];
			}
			if (!alive || generation !== screenGeneration) return;

			let at = 0;
			for (const item of toCheck) {
				const verified: { colStart: number; colEnd: number }[] = [];
				for (const c of item.candidates) {
					if (resolved[at]) verified.push({ colStart: c.colStart, colEnd: c.colEnd });
					at++;
				}
				if (fileLinkCache.size >= FILE_LINK_CACHE_MAX) {
					const oldest = fileLinkCache.keys().next().value;
					if (oldest !== undefined) fileLinkCache.delete(oldest);
				}
				fileLinkCache.set(item.text, { spans: verified.length > 0 ? verified : null, ts: Date.now() });
				if (verified.length > 0) anyFound = true;
			}
		}

		// Multi-row pass: detect web (http/https) + file:// URLs spanning
		// soft-wrapped rows. Each full-width row is a wrap candidate; the joined
		// logical line is scanned and any match that crosses a row boundary has
		// its per-row spans recorded in wrappedLinkSpans (merged by scanRowForLinks
		// so they survive the per-frame rebuild). Rebuilt fresh each pass.
		wrappedLinkSpans.clear();
		// Record a match's per-row spans into wrappedLinkSpans.
		const recordWrappedSpans = (startRow: number, matchIndex: number, matchEnd: number) => {
			for (let offset = matchIndex; offset < matchEnd; ) {
				const spanRow = startRow + Math.floor(offset / cols);
				const spanColStart = offset % cols;
				const remaining = matchEnd - offset;
				const spanColEnd = Math.min(spanColStart + remaining, cols);
				const existing = wrappedLinkSpans.get(spanRow) || [];
				existing.push({ colStart: spanColStart, colEnd: spanColEnd });
				wrappedLinkSpans.set(spanRow, existing);
				offset += spanColEnd - spanColStart;
			}
		};
		const spansMultipleRows = (matchIndex: number, matchEnd: number) =>
			Math.floor(matchIndex / cols) !== Math.floor((matchEnd - 1) / cols);
		const checkedLogicalStarts = new Set<number>();
		for (let i = 0; i < maxRow; i++) {
			if (!alive) return;
			const row = rowMap.get(i);
			if (!row) continue;
			const text = rowToText(row);
			if (text.length < cols) continue; // not full-width, not wrapped
			const hasFile = text.includes("file://");
			const hasWeb = /https?:\/\//.test(text);
			if (!hasFile && !hasWeb) continue;
			if (checkedLogicalStarts.has(i)) continue;
			try {
				const [startRow, logicalText] = (await ref("terminal_get_logical_line", {
					sessionId: props.sessionId,
					row: i,
				})) as [number, string];
				if (!alive || generation !== screenGeneration) return;
				if (startRow === i && logicalText === text) continue; // single row
				if (checkedLogicalStarts.has(startRow)) continue;
				checkedLogicalStarts.add(startRow);

				// Web URLs — no path resolution needed.
				for (const url of matchWebUrls(logicalText)) {
					const matchEnd = url.index + url.text.length;
					if (!spansMultipleRows(url.index, matchEnd)) continue;
					recordWrappedSpans(startRow, url.index, matchEnd);
					anyFound = true;
				}

				// file:// URLs — underline only once resolved to a real path.
				FILE_URL_RE.lastIndex = 0;
				let m: RegExpExecArray | null;
				while ((m = FILE_URL_RE.exec(logicalText)) !== null) {
					const matchEnd = m.index + m[0].length;
					if (!spansMultipleRows(m.index, matchEnd)) continue;
					try {
						const r = (await ref("resolve_terminal_path", { cwd, candidate: m[1] })) as {
							absolute_path: string;
							is_directory: boolean;
						} | null;
						if (!alive || generation !== screenGeneration) return;
						if (!r) continue;
						recordWrappedSpans(startRow, m.index, matchEnd);
						anyFound = true;
					} catch {
						/* resolve failed */
					}
				}
			} catch {
				/* terminal_get_logical_line not available */
				break;
			}
		}

		if (anyFound) {
			if (generation !== screenGeneration) return;
			for (let i = 0; i < maxRow; i++) scanRowForLinks(i);
			scheduleRepaint();
		}
	}

	let linkThrottle: ReturnType<typeof setTimeout> | undefined;

	async function checkLinksAtRow(row: number, col: number) {
		const ref = invokeRef;
		if (!ref || !alive) return;
		const gen = linkController.beginCheck();

		// OSC 8 hyperlinks take priority — the program explicitly tagged this cell
		try {
			const span = (await ref("terminal_hyperlink_span", {
				sessionId: props.sessionId,
				row,
				col,
			})) as [number, number, string] | null;
			if (span) {
				const [colStart, colEnd, uri] = span;
				let resolvedPath = uri;
				if (!uri.startsWith("http://") && !uri.startsWith("https://")) {
					const raw = uri.startsWith("file://") ? uri.slice(7) : uri;
					const termId = terminalsStore.getTerminalForSession(props.sessionId);
					const termData = termId ? terminalsStore.get(termId) : undefined;
					const cwd = termData?.cwd || "";
					try {
						const r = (await ref("resolve_terminal_path", { cwd, candidate: raw })) as {
							absolute_path: string;
							is_directory: boolean;
						} | null;
						if (r) resolvedPath = r.absolute_path;
					} catch {
						/* resolve failed — use raw URI */
					}
				}
				if (!alive || !linkController.isCurrent(gen)) return;
				hoveredLink = { row, colStart, colEnd, path: resolvedPath };
				canvasRef.style.cursor = currentLinkVisuals().pointer ? "pointer" : "text";
				if (currentFrame) {
					const m = metrics();
					if (m) repaintOverlay(currentFrame, m);
				}
				return;
			}
		} catch {
			/* ignore — command may not exist on older backend */
		}
		if (!alive || !linkController.isCurrent(gen)) return;

		let rowText: string;
		try {
			rowText = (await ref("terminal_get_row_text", {
				sessionId: props.sessionId,
				row,
			})) as string;
		} catch {
			return;
		}
		if (!alive || !linkController.isCurrent(gen)) return;

		const cacheKey = `${row}:${rowText}`;
		let links = linkCache.get(cacheKey);
		if (links === undefined) {
			const fpRe = FILE_PATH_RE;
			const fuRe = FILE_URL_RE;
			const fileMatches: { text: string; candidate: string; index: number }[] = [];
			const urlMatches: { text: string; path: string; index: number }[] = [];
			let match: RegExpExecArray | null;

			// Web URLs (no resolution needed). A URL that reaches the row's right
			// edge may be soft-wrapped onto the next row — defer it to the
			// logical-line pass below so the FULL url is captured, not the
			// truncated single-row prefix (e.g. http://127.0.0.1:8090 wrapping to
			// ".../8" + "090" must not open as http://127.0.0.1:8).
			for (const url of matchWebUrls(rowText)) {
				if (url.index + url.text.length >= rowText.length) continue;
				urlMatches.push({ text: url.text, path: url.text, index: url.index });
			}

			// File paths
			fpRe.lastIndex = 0;
			while ((match = fpRe.exec(rowText)) !== null) {
				const idx = rowText.indexOf(match[1], match.index);
				fileMatches.push({ text: match[1], candidate: match[1], index: idx });
			}
			fuRe.lastIndex = 0;
			while ((match = fuRe.exec(rowText)) !== null) {
				fileMatches.push({ text: match[0], candidate: match[1], index: match.index });
			}

			if (fileMatches.length === 0 && urlMatches.length === 0) {
				linkCache.set(cacheKey, null);
				if (linkCache.size > 200) {
					const oldest = linkCache.keys().next().value;
					if (oldest !== undefined) linkCache.delete(oldest);
				}
				links = null;
			} else {
				// Resolve file paths
				const termId = terminalsStore.getTerminalForSession(props.sessionId);
				const termData = termId ? terminalsStore.get(termId) : undefined;
				const cwd = termData?.cwd || "";
				const resolvedFiles = await Promise.all(
					fileMatches.map(async (m) => {
						try {
							const r = (await ref("resolve_terminal_path", { cwd, candidate: m.candidate })) as {
								absolute_path: string;
								is_directory: boolean;
							} | null;
							if (!r) return null;
							let line: number | undefined;
							let col: number | undefined;
							const lc = m.candidate.match(/:(\d+)(?::(\d+))?$/);
							if (lc) {
								line = parseInt(lc[1], 10);
								if (lc[2]) col = parseInt(lc[2], 10);
							}
							return { text: m.text, path: r.absolute_path, line, col, index: m.index };
						} catch (e) {
							appLogger.debug("terminal", "resolve_terminal_path failed", { candidate: m.candidate, error: e });
							return null;
						}
					}),
				);
				const validFiles = resolvedFiles.filter(Boolean) as {
					text: string;
					path: string;
					line?: number;
					col?: number;
					index: number;
				}[];
				const allLinks = [...validFiles, ...urlMatches];
				links = allLinks.length > 0 ? allLinks : null;
				if (linkCache.size > 200) {
					const oldest = linkCache.keys().next().value;
					if (oldest !== undefined) linkCache.delete(oldest);
				}
				linkCache.set(cacheKey, links);
			}
		}

		hoveredLink = null;
		if (links) {
			for (const link of links) {
				const start = link.index ?? 0;
				const end = start + link.text.length;
				if (col >= start && col < end) {
					hoveredLink = { row, colStart: start, colEnd: end, path: link.path, line: link.line, col: link.col };
					break;
				}
			}
		}

		// A web URL that reaches the row's right edge was deferred above (it may be
		// soft-wrapped); force the logical-line pass so it's re-detected on the
		// joined line even when the row happens not to be wrapped.
		const rowHasEdgeUrl = matchWebUrls(rowText).some((url) => url.index + url.text.length >= rowText.length);

		// If no single-row link found, try logical line (joins soft-wrapped rows)
		if (!hoveredLink && ref) {
			try {
				const [startRow, logicalText] = (await ref("terminal_get_logical_line", {
					sessionId: props.sessionId,
					row,
				})) as [number, string];
				if (!alive || !linkController.isCurrent(gen)) return;
				if (startRow !== row || logicalText !== rowText || rowHasEdgeUrl) {
					const cols = lastScreenCols > 0 ? lastScreenCols : currentFrame?.screenCols || 80;
					const colOffset = (row - startRow) * cols;
					const logicalCol = colOffset + col;
					const fuRe = FILE_URL_RE;
					const fpRe = FILE_PATH_RE;
					const logicalMatches: { text: string; candidate: string; index: number; isUrl: boolean }[] = [];

					fuRe.lastIndex = 0;
					let m: RegExpExecArray | null;
					while ((m = fuRe.exec(logicalText)) !== null) {
						logicalMatches.push({ text: m[0], candidate: m[1], index: m.index, isUrl: false });
					}
					fpRe.lastIndex = 0;
					while ((m = fpRe.exec(logicalText)) !== null) {
						const idx = logicalText.indexOf(m[1], m.index);
						logicalMatches.push({ text: m[1], candidate: m[1], index: idx, isUrl: false });
					}
					for (const url of matchWebUrls(logicalText)) {
						logicalMatches.push({ text: url.text, candidate: url.text, index: url.index, isUrl: true });
					}

					for (const lm of logicalMatches) {
						const matchEnd = lm.index + lm.text.length;
						if (logicalCol >= lm.index && logicalCol < matchEnd) {
							let resolvedPath = lm.candidate;
							if (!lm.isUrl) {
								const termId = terminalsStore.getTerminalForSession(props.sessionId);
								const termData = termId ? terminalsStore.get(termId) : undefined;
								const cwd = termData?.cwd || "";
								const r = (await ref("resolve_terminal_path", { cwd, candidate: lm.candidate })) as {
									absolute_path: string;
									is_directory: boolean;
								} | null;
								if (!alive || !linkController.isCurrent(gen)) return;
								if (!r) break;
								resolvedPath = r.absolute_path;
							}
							// Build multi-row spans
							const spans: { row: number; colStart: number; colEnd: number }[] = [];
							for (let offset = lm.index; offset < matchEnd; ) {
								const spanRow = startRow + Math.floor(offset / cols);
								const spanColStart = offset % cols;
								const remaining = matchEnd - offset;
								const spanColEnd = Math.min(spanColStart + remaining, cols);
								spans.push({ row: spanRow, colStart: spanColStart, colEnd: spanColEnd });
								offset += spanColEnd - spanColStart;
							}
							const firstSpan = spans[0];
							hoveredLink = {
								row: firstSpan.row,
								colStart: firstSpan.colStart,
								colEnd: firstSpan.colEnd,
								path: resolvedPath,
								spans,
							};
							break;
						}
					}
				}
			} catch {
				/* terminal_get_logical_line not available */
			}
		}

		canvasRef.style.cursor = hoveredLink && currentLinkVisuals().pointer ? "pointer" : "text";
		if (currentFrame) {
			const m = metrics();
			if (m) repaintOverlay(currentFrame, m);
		}
	}

	let scrollGestureEndTimer: ReturnType<typeof setTimeout> | undefined;

	onMount(async () => {
		initLinkModifier();
		const overlayCtx = overlayCanvasRef.getContext("2d");
		if (!overlayCtx) {
			appLogger.error("terminal", "Failed to acquire overlay 2D context");
			return;
		}
		octx = overlayCtx;

		const baseCtx = canvasRef.getContext("2d", { alpha: false });
		if (!baseCtx) {
			appLogger.error("terminal", "Failed to acquire canvas 2D context");
			return;
		}
		ctx = baseCtx;
		gridRenderer = createGridRenderer(ctx, {
			fontWeight: () => settingsStore.state.fontWeight,
			getFontFamily: () => settingsStore.getFontFamily(),
		});
		installFrameTimingDebugHook();
		acquireCache();

		// Session events FIRST — before the font load below, not after the whole
		// setup. These are fire-and-forget pushes: an OSC 133 prompt marker or a
		// watcher line that lands while its listener is missing is gone, and the
		// shell prints its first prompt as soon as the PTY spawns, while
		// `document.fonts.load` may still have a cold-cache round trip to make.
		// The grid subscription stays below: frames replay on subscribe.
		transport = createTransport(props.sessionId);
		invokeRef = (cmd, args) => transport!.invoke(cmd, args);
		// Teardown covering what exists right now: unmounting during the font load
		// below must not leave these listeners attached. Widened to the DOM
		// listeners further down, once those exist.
		unsubscribe = () => transport?.unsubscribe();
		try {
			// One shape on both transports: the Tauri event and the WS frame both
			// carry `{ cwd }`, so there is nothing to normalise here.
			await transport.onEvent("cwd", (payload) => {
				const { cwd } = payload as { cwd: string };
				terminalsStore.update(props.terminalId, { cwd });
				// A cd out of one repo and into another changes the answer to "who owns
				// this terminal?", and the sidebar renders that answer. Without this the
				// cwd updated but the tab stayed filed under the repo it started in.
				reconcileTerminalOwnership(props.terminalId);
				props.onCwdChange?.(props.terminalId, cwd);
			});
			await transport.onEvent("osc133", (payload) => {
				const { marker, line, exit_code } = payload as { marker: string; line: number; exit_code: number | null };
				terminalsStore.handleOsc133(props.terminalId, marker, line, exit_code ?? undefined);
			});
			// Lines assembled by the Rust reader, carrying the ids it already
			// matched. No raw stream is scanned here any more.
			await transport.onEvent("watcher-lines", (payload) => {
				const { lines } = payload as { lines: Array<{ text: string; matched_ids: string[] }> };
				pluginRegistry.handleWatcherLines(props.sessionId, lines);
			});
		} catch (e) {
			appLogger.error("terminal", "Failed to subscribe to terminal session events", {
				sessionId: props.sessionId,
				error: e,
			});
		}
		if (!alive) {
			transport.unsubscribe();
			return;
		}

		const fontFamily = settingsStore.getFontFamily();
		const fontSize = settingsStore.state.defaultFontSize;
		const fontWeight = settingsStore.state.fontWeight;
		await Promise.all([
			document.fonts.load(`${fontWeight} ${fontSize}px ${fontFamily}`, "M"),
			// The 2nd arg is not decorative: fonts.load() only schedules the faces
			// covering the characters in it — "" matches nothing and downloads
			// nothing. Canvas 2D never pulls a webfont in by itself, so without a
			// real powerline glyph here U+E0B0 & co. render with a system fallback.
			document.fonts.load(`400 ${fontSize}px "Symbols Nerd Font Mono"`, "\ue0b0"),
		]).catch(() => document.fonts.ready);
		// Unmounted while the fonts were loading. onCleanup has already run — it
		// tore down the transport, the only thing installed at that point — and
		// everything below installs observers and document listeners, then
		// overwrites `unsubscribe` with a disposer nobody will call again. Without
		// this the disposed component is retained for the page lifetime and its
		// callbacks keep firing. Font loading is exactly the window a repo switch
		// or a fast tab close lands in.
		if (!alive) return;
		remeasure();

		resizeObserver = new ResizeObserver(() => {
			clearTimeout(resizeDebounce);
			resizeDebounce = setTimeout(() => remeasure(), 100);
		});
		resizeObserver.observe(containerRef);

		// Flow control: ack on a trailing timer when hidden, request full frame on show
		visibilityObserver = new IntersectionObserver(
			(entries) => {
				const isVisible = entries[0]?.isIntersecting ?? false;
				if (isVisible && hidden) {
					hidden = false;
					// Back to full rate: clear the gate now instead of waiting out the
					// hidden interval, so the frame requested below is not queued behind it.
					hiddenAck.cancel();
					ackFrame();
					fullRepaintNeeded = true;
					lastDisplayOffset = -1;
					// Freeze-investigation: hidden→visible is the repo-switch show path.
					// Breadcrumb + burst note expose the un-staggered thundering herd.
					markPerf("term.show", { sessionId: props.sessionId });
					// Don't clear rowMap/currentFrame here — keep showing the
					// last painted content until the fresh frame arrives.
					// onFrame() replaces rowMap when a full frame arrives
					// (rows.length >= screenRowCount), so stale data is
					// naturally discarded without a blank flash.
					remeasure();
					if (focused()) startBlink();
					// If remeasure saw 0x0 (layout not yet computed after
					// display:none → display:block), retry after a frame.
					const rect = containerRef.getBoundingClientRect();
					if (rect.width <= 0 || rect.height <= 0) {
						requestAnimationFrame(() => {
							remeasure();
							noteFrameRequest();
							invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(
								ipcErr("terminal_request_frame"),
							);
						});
					} else {
						noteFrameRequest();
						invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(
							ipcErr("terminal_request_frame"),
						);
					}
				} else if (!isVisible && !hidden) {
					hidden = true;
					stopBlink();
					// Shrink to free the backing store while hidden.
					canvasRef.width = 1;
					canvasRef.height = 1;
					overlayCanvasRef.width = 1;
					overlayCanvasRef.height = 1;
					rowMap.clear();
					fileLinkCache.clear();
				}
			},
			{ threshold: 0 },
		);
		visibilityObserver.observe(containerRef);

		// DPR change: browser zoom or external monitor switch
		dprChangeHandler = () => {
			dprMediaQuery?.removeEventListener("change", dprChangeHandler!);
			dprMediaQuery = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
			dprMediaQuery.addEventListener("change", dprChangeHandler!);
			remeasure();
		};
		dprMediaQuery = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
		dprMediaQuery.addEventListener("change", dprChangeHandler);

		// Keyboard input is routed through keyInputRef (a hidden <input>) so that
		// macOS dead-key composition and IME work correctly. Canvas elements in
		// WKWebView don't fully participate in the macOS text input system, so dead
		// keys (quotes, accents, etc.) fail when keydown listeners live on the canvas.
		// When the canvas gains focus, we redirect to keyInputRef.
		bindings.listen(canvasRef, "focus", () => {
			keyInputRef.focus({ preventScroll: true });
		});

		// iOS/iPadOS soft keyboards stop auto-repeating Backspace once the focused
		// field is empty, so holding Delete erased only a single character. Keep a
		// small space buffer in the hidden input on touch devices so
		// each key-repeat tick has something to delete and keeps firing
		// deleteContent* events. Desktop keeps the field empty so macOS dead-key
		// composition (which needs an empty input) is unaffected.
		const INPUT_BUFFER = "   ";
		const resetInputBuffer = () => {
			if (isTouchDevice) {
				keyInputRef.value = INPUT_BUFFER;
				try {
					keyInputRef.setSelectionRange(INPUT_BUFFER.length, INPUT_BUFFER.length);
				} catch {
					// setSelectionRange can throw on a hidden/detached input — harmless
				}
			} else {
				keyInputRef.value = "";
			}
		};

		bindings.listen(keyInputRef, "focus", () => {
			setFocused(true);
			startBlink();
			props.onFocus?.();
			resetInputBuffer();
			if (currentFrame?.focusReporting) writePtyNoScroll("\x1b[I");
		});
		bindings.listen(keyInputRef, "blur", () => {
			setFocused(false);
			stopBlink();
			repaintCursorIfNeeded();
			if (currentFrame?.focusReporting) writePtyNoScroll("\x1b[O");
			// A wheel gesture that was mid-flight when focus left this pane must not
			// resume it on refocus.
			resetWheelNotch(wheelNotch);
		});

		// Text from input methods that don't emit usable keydown events — iOS/
		// iPadOS soft keyboard, dictation, and predictive/autocorrect — arrives
		// only as `input` events (keydown fires with key "Unidentified", so
		// keyToSequence returns null and never preventDefaults). On desktop,
		// printable keys are handled in keydown with preventDefault(), so no
		// `input` event fires for them; anything that reaches here is mobile-style
		// text we must forward to the PTY ourselves. During composition the input
		// must hold the in-progress text or compositionend will never resolve, so
		// leave it untouched in that case.
		// DEFERRED (2026-06-27) — verify iOS dictation interim/replacement edge
		// cases on a real iPad: with autocorrect off the common path is
		// incremental insertText, but some iOS versions emit insertReplacementText
		// which could double-write. Needs device testing to confirm.
		bindings.listen(keyInputRef, "input", (e) => {
			const ie = e as InputEvent;
			if (ie.isComposing) return;
			switch (ie.inputType) {
				case "deleteContentBackward":
					writePty("\x7f");
					break;
				case "deleteContentForward":
					writePty("\x1b[3~");
					break;
				case "insertLineBreak":
				case "insertParagraph":
					writePty("\r");
					break;
				default:
					// insertText, insertReplacementText, insertFromDictation, …
					if (ie.data) writePty(ie.data);
			}
			resetInputBuffer();
		});

		// --- Keyboard ---
		const composition = createCompositionState();
		bindings.listen(keyInputRef, "compositionstart", () => {
			const m = metrics();
			if (currentFrame && m) syncImePosition(currentFrame.cursorRow, currentFrame.cursorCol, m);
		});
		bindings.listen(keyInputRef, "compositionend", (e) => {
			const data = composition.onCompositionEnd(e.data);
			if (data) writePty(data);
			queueMicrotask(() => {
				resetInputBuffer();
			});
		});

		let leftOptionHeld = false;

		bindings.listen(keyInputRef, "keydown", (e: KeyboardEvent) => {
			if (composition.shouldSuppressKeydown(e.isComposing, e.key)) {
				e.preventDefault();
				return;
			}
			resetBlink();

			if (settingsStore.state.blockTimestampMode === "modifier" && e.ctrlKey && e.metaKey && !blockTimestampsVisible) {
				blockTimestampsVisible = true;
				fullRepaintNeeded = true;
				if (currentFrame && metrics()) paintFrame(currentFrame, metrics()!);
			}

			// Arrow Down with no modifiers: snap to bottom when scrolled up
			if (
				e.key === "ArrowDown" &&
				!e.shiftKey &&
				!e.ctrlKey &&
				!e.metaKey &&
				!e.altKey &&
				currentFrame &&
				currentFrame.displayOffset > 0
			) {
				e.preventDefault();
				invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta: -currentFrame.displayOffset }).catch(
					ipcErr("terminal_scroll"),
				);
				return;
			}

			// Force re-render: clear accumulated buffer and request fresh frame from Rust
			if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "l" && !e.altKey) {
				e.preventDefault();

				rowMap.clear();
				clearDetectedLinks();
				fullRepaintNeeded = true;
				currentFrame = null;
				lastDisplayOffset = -1;
				remeasure();
				invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
				return;
			}

			if ((e.metaKey || e.ctrlKey) && e.key === "f" && !e.altKey && !e.shiftKey) {
				e.preventDefault();
				props.onSearchOpen?.();
				return;
			}

			// Escape closes search when visible
			if (e.key === "Escape" && props.searchVisible) {
				e.preventDefault();
				props.onSearchClose?.();
				return;
			}

			// Resume banner: Space/Enter accept, other keys dismiss
			if (props.hasPendingResume) {
				if (e.key === " " || e.key === "Enter") {
					e.preventDefault();
					props.onResume?.();
				} else if (e.key.length === 1) {
					// preventDefault stops the hidden key-input from also emitting an
					// `input` event for this printable key — without it the char is
					// written twice (keydown + input), e.g. "c" → "cc" (issue: resume
					// banner double-echo). Mirrors the normal printable path below.
					e.preventDefault();
					props.onResumeDismiss?.();
					// Let the keystroke pass through to PTY
					// macOS Right Option: send composed char directly, skip ESC prefix
					if (isMacOS() && e.altKey && !leftOptionHeld) {
						writePty(e.key);
					} else {
						const seq = keyToSequence(e, currentFrame?.appCursor ?? false);
						if (seq !== null) writePty(seq);
					}
				} else if (e.key === "Escape" || e.key === "Backspace" || e.key === "Delete" || e.key === "Tab") {
					e.preventDefault();
					props.onResumeDismiss?.();
				}
				return;
			}

			// Cmd+Enter: don't send \r to PTY — let document-level keybinding handle
			if (e.metaKey && e.key === "Enter") {
				return;
			}

			// Copy selection with the platform copy modifier (Cmd on macOS, Ctrl on Win/Linux).
			// On macOS Ctrl+C is the interrupt key — distinct from Cmd+C — so it must NOT be
			// hijacked into copy here; it falls through to the Emacs Ctrl+letter path → \x03.
			// Also fires when coords were cleared by mouseup (auto-copy) but cache is still warm.
			const copyModifier = isMacOS() ? e.metaKey : e.ctrlKey;
			if (copyModifier && e.key.toLowerCase() === "c" && ((selection.start && selection.end) || selection.cachedText)) {
				e.preventDefault();
				e.stopPropagation();
				// Skip if the native Edit > Copy accelerator (menu.rs CmdOrCtrl+C) already fired for
				// this same keypress — otherwise we writeText() twice in <200ms, which macOS DeepL
				// reads as a double-Cmd+C and pops up its translation overlay. Same guard as
				// useKeyboardShortcuts.ts. The menu path (copyFromTerminal) handles the copy.
				if (Date.now() - lastMenuActionTime < 200) return;
				copySelection();
				return;
			}

			// Windows Ctrl+V paste
			if (isWindows() && e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey && (e.key === "v" || e.key === "V")) {
				e.preventDefault();
				navigator.clipboard
					.readText()
					.then((text) => {
						if (text) {
							if (currentFrame?.bracketedPaste) {
								writePty(`\x1b[200~${text}\x1b[201~`);
							} else {
								writePty(text);
							}
						}
					})
					.catch(ipcErr("clipboard_read"));
				return;
			}

			// Any keypress clears selection — full repaint to remove ghost highlights.
			// Skip modifier keys and Cmd+C/V so the chord completes before selection is dropped.
			// Include cachedSelectionText so a stale warm cache (coords already null) gets cleared
			// too — otherwise it sticks until a resize and keeps swallowing Ctrl+C into copy.
			if (
				(selection.start || selection.cachedText) &&
				e.key !== "Meta" &&
				e.key !== "Control" &&
				e.key !== "Alt" &&
				e.key !== "Shift" &&
				!((e.ctrlKey || e.metaKey) && (e.key.toLowerCase() === "c" || e.key.toLowerCase() === "v"))
			) {
				selection.clear();
				fullRepaintNeeded = true;
				scheduleRepaint();
			}

			// Shift+Enter → ESC CR (multi-line for Claude Code, Ink, etc.)
			// Must run BEFORE Kitty block — CC expects \x1b\r, not CSI 13;2 u
			if (e.key === "Enter" && e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey) {
				e.preventDefault();
				writePty("\x1b\r");
				return;
			}

			// Shift+Tab: send CSI Z but prevent browser focus navigation
			if (e.key === "Tab" && e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey) {
				e.preventDefault();
				writePty("\x1b[Z");
				return;
			}

			// macOS WebKit Emacs keybindings: Ctrl+A/D/E/K etc. intercepted by native
			// text system before our handler. Use e.code for reliable mapping.
			if (isMacOS() && e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
				const cm = e.code.match(/^Key([A-Z])$/);
				if (cm) {
					const ctrl = String.fromCharCode(cm[1].charCodeAt(0) - 0x40);
					e.preventDefault();
					writePty(ctrl);
					return;
				}
			}

			// Kitty keyboard protocol: encode special keys when flag 1 (disambiguate) is active
			const kbFlags = currentFrame?.keyboardFlags ?? 0;
			if (kbFlags & 1) {
				const seq = kittySequenceForKey(e.key, e.shiftKey, e.altKey, e.ctrlKey, e.metaKey);
				if (seq !== null) {
					e.preventDefault();
					writePty(seq);
					return;
				}
			}

			// macOS Alt/Option key handling
			// Left Option → ESC sequences (word-jump, backward-kill-word, etc.)
			// Right Option → compose characters (~ @ # [ ] { } on international keyboards)
			if (isMacOS() && e.altKey && !e.metaKey && !e.ctrlKey) {
				if (e.code === "AltLeft") {
					leftOptionHeld = true;
					return;
				}
				if (leftOptionHeld) {
					const altSeq = altSequenceFromCode(e);
					if (altSeq) {
						e.preventDefault();
						writePty(altSeq);
						return;
					}
				}
				// Right Option: send the composed character directly (e.g. ~ @ # [ ])
				if (!leftOptionHeld && e.key.length === 1) {
					e.preventDefault();
					writePty(e.key);
					return;
				}
			}
			if (!e.altKey) leftOptionHeld = false;

			// Ctrl+Shift+. is the Windows/Linux form of the global block-fold-toggle
			// shortcut (Cmd+Shift+. on macOS, where e.metaKey already short-circuits
			// keyToSequence below). Without this, the printable-character fallback
			// would swallow it as a literal "." keystroke instead of letting it
			// bubble to the document-level shortcut listener.
			if (isGlobalShortcutPassthrough(e)) {
				return;
			}

			// Default: legacy VT100 encoding
			const seq = keyToSequence(e, currentFrame?.appCursor ?? false);
			if (seq !== null) {
				e.preventDefault();
				e.stopPropagation();
				writePty(seq);
			}
		});

		// Track Alt key release for macOS left-option state
		bindings.listen(keyInputRef, "keyup", (e: KeyboardEvent) => {
			if (e.code === "AltLeft") leftOptionHeld = false;
			if (blockTimestampsVisible && (!e.ctrlKey || !e.metaKey)) {
				blockTimestampsVisible = false;
				fullRepaintNeeded = true;
				if (currentFrame && metrics()) paintFrame(currentFrame, metrics()!);
			}
		});

		bindings.listen(keyInputRef, "paste", (e: ClipboardEvent) => {
			if (e.clipboardData) {
				const items = e.clipboardData.items;
				for (let i = 0; i < items.length; i++) {
					if (items[i].type.startsWith("image/")) {
						e.preventDefault();
						writePty("\x16");
						return;
					}
				}
			}
			const text = e.clipboardData?.getData("text");
			if (text) {
				if (currentFrame?.bracketedPaste) {
					writePty(`\x1b[200~${text}\x1b[201~`);
				} else {
					writePty(text);
				}
			}
			e.preventDefault();
		});

		// --- Mouse selection ---
		const clickCounter: ClickCounterState = { count: 0, lastClickTime: 0 };
		/** Buttons this canvas reported down, so their release is owed to the app. */
		const reportedDown = new Set<number>();
		// Set alongside selection.mode at mousedown (word/line respectively); the
		// fixed span flushSelectionDrag must keep fully included no matter which
		// way the drag goes. Only read while selection.mode says to read them, so
		// staleness across gestures can't leak in.
		let wordAnchor: { row: number; left: number; right: number } | null = null;
		let lineAnchorRow: number | null = null;

		// Rebuilt only when the underlying settings actually change (regex mode
		// recompiles every alternate) — cheap key comparison on every call
		// otherwise. Falls back to the plain `wordBoundsAt` import when the user
		// is on "characters" mode with the default separator string, so the
		// zero-configuration case pays no indirection cost either.
		let cachedWordResolverKey = "";
		let cachedWordResolver: WordBoundaryFn = wordBoundsAt;
		function getWordBoundaryResolver(): WordBoundaryFn {
			const mode = settingsStore.state.wordSelectionMode;
			const separators = mode === "characters" ? settingsStore.state.wordSeparators : "";
			const regexAlternates = mode === "regex" ? settingsStore.state.wordSelectionRegex : "";
			const key = `${mode}\u0000${separators}\u0000${regexAlternates}`;
			if (key !== cachedWordResolverKey) {
				cachedWordResolverKey = key;
				cachedWordResolver = createWordBoundaryResolver({ mode, separators, regexAlternates });
			}
			return cachedWordResolver;
		}

		/** Row accessor for `buildSmartSelectionWindow` — absolute row → viewport row → rowMap. */
		function getRowByAbs(absRow: number): DecodedRow | null {
			const vpRow = absRowToViewport(absRow);
			return vpRow !== null ? (rowMap.get(vpRow) ?? null) : null;
		}

		/** Try smart-selection matching at a click position; null if no rules
		 *  configured/enabled, or nothing matched (caller falls back to
		 *  word-boundary selection in every case). Always attempted — there is
		 *  no master on/off switch; "Double-click performs" only controls
		 *  what double-click itself does, not quad-click or the context menu. */
		function trySmartMatch(absRow: number, col: number): ResolvedSmartMatch | null {
			const win = buildSmartSelectionWindow(absRow, col, SMART_SELECTION_RADIUS, getRowByAbs);
			if (win.targetOffset < 0) return null;
			const rules = resolveSmartSelectionRules(settingsStore.state.smartSelectionRules);
			const match = findSmartMatch(win.text, win.targetOffset, rules);
			if (!match) return null;
			// Map the match's text-offset span back to grid coordinates via the
			// window's per-character coordinate map.
			const startCoord = win.coords[match.startOffset];
			const endCoord = win.coords[match.endOffset - 1];
			return { ...match, startCoord, endCoord };
		}

		bindings.listen(canvasRef, "mousedown", (e: MouseEvent) => {
			keyInputRef.focus({ preventScroll: true });
			if (currentFrame && currentFrame.mouseMode > 0 && !e.shiftKey) {
				const pos = canvasToGrid(e);
				// Right-click on a link (UI-first, #57) and modifier+click on a link in
				// "modifier" mode both need to reach their click-handling instead of
				// being forwarded as a mouse-report press — see shouldSkipMouseReportForLink.
				const onLink = detectedLinks.get(pos.row)?.some((sp) => pos.col >= sp.colStart && pos.col < sp.colEnd) ?? false;
				if (shouldSkipMouseReportForLink(settingsStore.state.linkActivation, e.button, isLinkModifier(e), onLink)) {
					return;
				}
				if (currentFrame.sgrMouse) {
					writePtyNoScroll(sgrMouseSequence(e.button, pos.col, pos.row, true, e));
					reportedDown.add(e.button);
				}
				e.preventDefault();
				return;
			}
			if (e.button !== 0) return;
			const pos = canvasToGrid(e);
			const absRow = viewportRowToAbs(pos.row);
			if (absRow === null) return;

			// Gutter click: fold chevron zone toggles fold, the rest of the block's
			// gutter run selects its output for copying (issue #6 — there used to be
			// no fold gesture here at all; this is the split that adds one without
			// taking away the existing copy behavior).
			{
				const rect = canvasRef.getBoundingClientRect();
				const rawX = e.clientX - rect.left;
				if (rawX < GUTTER_PX) {
					const term = terminalsStore.get(props.terminalId);
					if (term) {
						const allBlocks = [...term.commandBlocks, term.activeBlock].filter(
							Boolean,
						) as import("../../stores/terminals").CommandBlock[];
						const block = findBlockAtViewport(allBlocks, absRow, 0);
						if (block) {
							// Only treat the header row as a fold target when folding is
							// enabled — otherwise it falls through to the copy behavior
							// below, same as it did before this row had a fold zone at all.
							if (settingsStore.state.blockFoldingEnabled && gutterZoneAt(absRow, block) === "fold") {
								toggleFoldForBlock(block);
								e.preventDefault();
								return;
							}
							const startRow = (block.executionLine ?? block.promptLine) + 1;
							const endRow = (block.endLine ?? absRow) - 1;
							if (endRow >= startRow) {
								selection.start = { row: startRow, col: 0 };
								selection.end = { row: endRow, col: lastResizeCols - 1 };
								selection.selecting = false;
								fullRepaintNeeded = true;
								scheduleRepaint();
								e.preventDefault();
								return;
							}
						}
					}
				}
			}

			const absPos = { col: pos.col, row: absRow };

			// Shift+click: extend selection from existing anchor
			if (e.shiftKey && selection.start) {
				selection.end = absPos;
				selection.selecting = true;
				selection.mode = "char";
				fullRepaintNeeded = true;
				scheduleRepaint();
				return;
			}

			const { kind: clickKind } = classifyClick(Date.now(), clickCounter);

			// Double-click tries smart selection first when the user has opted in
			// ("smart" double-click action); quad-click always tries it. Either
			// way, no match falls through to the same word/line logic as before —
			// see decideMousedownSelection's doc comment.
			const tryingSmart =
				(clickKind === "double" && settingsStore.state.doubleClickAction === "smart") || clickKind === "quad";
			const smartMatch = tryingSmart ? trySmartMatch(absRow, pos.col) : null;

			if (smartMatch) {
				const singleRow = smartMatch.startCoord.row === smartMatch.endCoord.row;
				selection.start = smartMatch.startCoord;
				selection.end = smartMatch.endCoord;
				selection.mode = singleRow ? "word" : "char";
				wordAnchor = singleRow
					? { row: smartMatch.startCoord.row, left: smartMatch.startCoord.col, right: smartMatch.endCoord.col }
					: null;
				// Alt/Option+double-click runs the match's default action, if it has
				// one — checked here (mousedown) rather than the eventual `click`
				// event, since a double-click's 2nd mousedown is what carries the
				// click count to "double" in the first place.
				if (e.altKey && clickKind === "double") {
					const defaultAction = smartMatch.rule.actions.find((a) => a.isDefault);
					if (defaultAction) runSmartAction(defaultAction, smartMatch);
				}
			} else {
				const vpRow = absRowToViewport(absRow);
				const row = vpRow !== null ? rowMap.get(vpRow) : null;
				const wordBounds = clickKind === "double" && row ? getWordBoundaryResolver()(row, pos.col) : null;
				const maxCol =
					clickKind === "triple" || clickKind === "quad" ? lastGridColForRect(canvasRef.getBoundingClientRect()) : 0;
				const decision = decideMousedownSelection({ clickKind, absPos, wordBounds, maxCol });
				selection.start = decision.start;
				selection.end = decision.end;
				selection.mode = decision.mode;
				wordAnchor = decision.wordAnchor;
				if (decision.lineAnchorRow !== null) lineAnchorRow = decision.lineAnchorRow;
			}
			selection.selecting = true;
			// Cache the canvas rect for the whole drag — its position doesn't move
			// while selecting (auto-scroll shifts content offset, not the element).
			selectionDragRect = canvasRef.getBoundingClientRect();
			fullRepaintNeeded = true;
			scheduleRepaint();
		});

		// Coalesced selection-drag processing: runs at most once per frame with the
		// latest mouse position (#9b13). Uses the cached drag rect (no per-move gBCR).
		const flushSelectionDrag = () => {
			selectionRafId = undefined;
			const e = lastSelectionEvent;
			if (!e || !selection.selecting || !selection.start) return;
			const rect = selectionDragRect ?? canvasRef.getBoundingClientRect();
			const m = metrics();
			if (m) {
				const yAbove = rect.top - e.clientY;
				const yBelow = e.clientY - rect.bottom;
				if (yAbove > 0) {
					startSelectionScroll(Math.ceil(yAbove / m.cellHeight));
				} else if (yBelow > 0) {
					startSelectionScroll(-Math.ceil(yBelow / m.cellHeight));
				} else {
					stopSelectionScroll();
				}
			}
			const pos = canvasToGrid(e, rect);
			const absRow = viewportRowToAbs(pos.row);
			if (absRow === null) return;

			// Word/line drag extension: re-derive the boundary at the live drag
			// position each frame and union it with whichever edge of the
			// mousedown anchor (word or line) sits away from the drag direction —
			// see extendSelectionDrag's doc comment for why.
			const vpRow = absRowToViewport(absRow);
			const dragRow = vpRow !== null ? rowMap.get(vpRow) : null;
			const dragBounds = dragRow ? getWordBoundaryResolver()(dragRow, pos.col) : null;
			const extended = extendSelectionDrag(
				selection.mode,
				{ wordAnchor, lineAnchorRow },
				{ row: absRow, col: pos.col, bounds: dragBounds, maxCol: lastGridColForRect(rect) },
				selection.start ?? { row: absRow, col: pos.col },
			);
			selection.start = extended.start;
			selection.end = extended.end;
			const mRepaint = metrics();
			if (currentFrame && mRepaint) paintFrame(currentFrame, mRepaint);
		};

		const onMouseMove = (e: MouseEvent) => {
			// The listener is on document, so this fires for every terminal on every
			// move. A hidden one has a 1x1 canvas and canvasToGrid clamps, so without
			// this it reported cell (0,0) into a PTY the pointer never touched.
			if (hidden) return;
			// A gesture already claimed for local selection (mousedown decided this
			// one wasn't forwarded — see below) stays local for its whole lifetime,
			// even if the app's mouse-reporting mode flips mid-drag. Without this,
			// a long-enough drag could straddle a mode change and have its tail end
			// silently forwarded instead of extending the selection.
			if (
				currentFrame &&
				shouldForwardMouseGesture({
					selecting: selection.selecting,
					mouseMode: currentFrame.mouseMode,
					shiftKey: e.shiftKey,
				})
			) {
				const rect = canvasRef.getBoundingClientRect();
				if (!isPointerInsideRect(e, rect)) return;
				const motionButton = motionReportButton(currentFrame.mouseMode, e.buttons);
				if (motionButton !== null) {
					const pos = canvasToGrid(e, rect);
					writePtyNoScroll(sgrMouseSequence(32 + motionButton, pos.col, pos.row, true, e));
				}
				return;
			}

			// Gutter hover (issue #2): pointer cursor over the click-to-copy/fold
			// strip, with an early return so the "no link hovered" branch below can't
			// immediately clobber it back to "text".
			if (!selection.selecting) {
				const rect = canvasRef.getBoundingClientRect();
				const rawX = e.clientX - rect.left;
				if (rawX < GUTTER_PX && isPointerInsideRect(e, rect)) {
					if (hoveredLink) {
						hoveredLink = null;
						const m = metrics();
						if (currentFrame && m) repaintOverlay(currentFrame, m);
					}
					canvasRef.style.cursor = "pointer";
					return;
				}
			}

			// Selection drag: coalesce into one rAF/frame with the latest position.
			if (selection.selecting && selection.start) {
				lastSelectionEvent = e;
				if (selectionRafId === undefined) {
					selectionRafId = requestAnimationFrame(flushSelectionDrag);
				}
			}

			// Link detection (throttled) — see shouldResolveLinkHoverOnMove. Always
			// remember the position so the modifier-held effect below can resolve
			// it instantly on keydown, even when this move itself skipped resolution.
			if (!selection.selecting) {
				lastLinkHoverEvent = e;
				clearTimeout(linkThrottle);
				if (!shouldResolveLinkHoverOnMove(settingsStore.state.linkActivation, linkModifierHeld())) {
					if (hoveredLink) {
						hoveredLink = null;
						canvasRef.style.cursor = "text";
						const m = metrics();
						if (currentFrame && m) repaintOverlay(currentFrame, m);
					}
				} else {
					linkThrottle = setTimeout(() => {
						const pos = canvasToGrid(e);
						checkLinksAtRow(pos.row, pos.col);
					}, 100);
				}
			}
		};

		const onMouseUp = (e: MouseEvent) => {
			// A release we owe the application outlives the reasons to stay quiet:
			// hiding the terminal or dragging off it mid-gesture would otherwise
			// leave the button logically held with nothing left to retract it.
			const rect = canvasRef.getBoundingClientRect();
			const reportUp = shouldReportMouseUp(reportedDown, e.button, isPointerInsideRect(e, rect));
			const owed = reportedDown.delete(e.button);
			if (hidden && !owed) return;
			// Same latch as onMouseMove: once a gesture is a local selection, its
			// own mouseup must always run local teardown (stop autoscroll, copy,
			// clear selecting/rect/rAF) — never get swallowed by a mode flip that
			// happened mid-drag, which used to leave selecting/autoscroll/rAF
			// dangling until the next unrelated mousedown reset them.
			if (
				currentFrame &&
				shouldForwardMouseGesture({
					selecting: selection.selecting,
					mouseMode: currentFrame.mouseMode,
					shiftKey: e.shiftKey,
				})
			) {
				if (!reportUp) return;
				// canvasToGrid clamps to the grid, so a release outside the canvas
				// reports the edge cell — what a terminal does for a drag-out.
				const pos = canvasToGrid(e, rect);
				if (currentFrame.sgrMouse) {
					writePtyNoScroll(sgrMouseSequence(e.button, pos.col, pos.row, false, e));
				}
				return;
			}
			stopSelectionScroll();
			if (selection.selecting && selection.start && selection.end) {
				if (selection.hasRange()) {
					copySelection();
				} else {
					selection.start = null;
					selection.end = null;
					fullRepaintNeeded = true;
					scheduleRepaint();
				}
			}
			selection.selecting = false;
			// End the coalesced drag: drop the cached rect + any pending frame.
			selectionDragRect = null;
			lastSelectionEvent = null;
			if (selectionRafId !== undefined) {
				cancelAnimationFrame(selectionRafId);
				selectionRafId = undefined;
			}
		};

		bindings.listen(document, "mousemove", onMouseMove);
		bindings.listen(document, "mouseup", onMouseUp);

		// Link click — opens per the "Open links on" setting (click / modifier+click
		// / never), skip if user was selecting text.
		bindings.listen(canvasRef, "click", (e: MouseEvent) => {
			if (!hoveredLink) return;
			if (selection.hasRange()) return;
			if (!shouldOpenOnClick(settingsStore.state.linkActivation, isLinkModifier(e))) return;
			openLink(hoveredLink);
		});

		// Right-click on a detected link and/or a smart-selection rule match
		// (with actions — iTerm2's "actionRequired" mode) → context menu. Only
		// when at least one of the two applies; elsewhere the default is left
		// alone. Rule actions surface here regardless of `linkActivation` —
		// "never" only disables click-to-open, not the right-click menu.
		bindings.listen(canvasRef, "contextmenu", async (e: MouseEvent) => {
			const pos = canvasToGrid(e);
			const onLink = detectedLinks.get(pos.row)?.some((sp) => pos.col >= sp.colStart && pos.col < sp.colEnd);
			const absRow = viewportRowToAbs(pos.row);
			const smartMatch = absRow !== null ? trySmartMatch(absRow, pos.col) : null;
			const hasSmartActions = smartMatch && smartMatch.rule.actions.length > 0;
			if (!onLink && !hasSmartActions) return;
			e.preventDefault();
			// Stop the App-level terminal context menu (#terminal-panes onContextMenu)
			// from also opening and covering our menu.
			e.stopPropagation();
			setSmartMenuMatch(hasSmartActions ? smartMatch : null);
			if (onLink) {
				await checkLinksAtRow(pos.row, pos.col);
				setLinkMenuTarget(
					hoveredLink ? { path: hoveredLink.path, line: hoveredLink.line, col: hoveredLink.col } : null,
				);
			} else {
				// Not on a link this time — clear any stale target from a
				// previous right-click so a smart-only menu can't also show a
				// leftover Open/Copy-link pair.
				setLinkMenuTarget(null);
			}
			if (!linkMenuTarget() && !hasSmartActions) return;
			linkMenu.openAt(e.clientX, e.clientY);
		});

		// --- Scroll ---

		function handleWheel(e: WheelEvent) {
			e.preventDefault();
			e.stopPropagation();
			// While dragging the scrollbar thumb, ignore wheel input — otherwise it would
			// re-enter smooth-scroll (scrollPosF != null) and re-freeze repaints mid-drag.
			if (scrollDragging) return;
			// Forward the wheel to the app whenever it has mouse reporting enabled —
			// it owns the viewport and wants to drive its own scroll. This covers
			// both the alternate screen (vim, lazygit, htop) AND inline fullscreen
			// TUIs in the main buffer (e.g. `grok --no-alt-screen`, which scrolls its
			// own conversation on SGR wheel events — verified against grok 0.2.67).
			// We deliberately do NOT gate on historySize === 0: grok emits `\x1b[24S`
			// at startup, creating scrollback, so that proxy left grok's wheel dead
			// (TUIC scrolled its own history instead of forwarding to grok).
			// Shift+wheel always scrolls the TUIC scrollback, never the app — the
			// escape hatch matching the click/motion handlers' `!e.shiftKey` bypass.
			// (wheelScrollDelta inside quantizeWheelNotches/wheelDeltaToPixels also
			// corrects for macOS/WebKit reporting Shift+wheel on deltaX instead of
			// deltaY — without that, this escape hatch does nothing until Shift is
			// released and the momentum tail lands back on deltaY.)
			if (currentFrame && currentFrame.mouseMode > 0 && !e.shiftKey) {
				const ch = metrics()?.cellHeight ?? 20;
				const rows = lastResizeRows || 24;
				// Quantize to whole notches the way a native terminal does: macOS
				// delivers a wheel event for the entire momentum tail after a flick
				// (60-120 events/s of decaying deltas), and forwarding one SGR notch
				// per event — as this used to — turns one flick into dozens of
				// notches sent to the app.
				const notches = quantizeWheelNotches(wheelNotch, e, ch, rows);
				clearTimeout(scrollGestureEndTimer);
				scrollGestureEndTimer = setTimeout(onWheelGestureEnd, WHEEL_GESTURE_END_MS);
				if (notches === 0) return;
				const pos = canvasToGrid(e as unknown as MouseEvent);
				const btn = notches < 0 ? 64 : 65;
				const seq = sgrMouseSequence(btn, pos.col, pos.row, true, e as unknown as MouseEvent);
				// One write_pty per DOM event instead of one per notch — the sequence
				// is stateless, so repeating it is byte-identical to N separate writes.
				writePtyNoScroll(seq.repeat(Math.abs(notches)));
				return;
			}
			const dy = wheelDeltaToPixels(e, metrics()?.cellHeight ?? 20, lastResizeRows || 24);
			const atBottom =
				currentFrame && currentFrame.displayOffset === 0 && (scroll.position == null || scroll.position <= 0);
			const atTop =
				currentFrame &&
				currentFrame.displayOffset >= currentFrame.historySize &&
				(scroll.position == null || scroll.position >= currentFrame.historySize);
			if ((atBottom && dy > 0) || (atTop && dy < 0)) return;

			handleScrollDelta(dy);

			clearTimeout(scrollGestureEndTimer);
			scrollGestureEndTimer = setTimeout(resetScrollGesture, 200);
		}
		bindings.listen(canvasRef, "wheel", handleWheel, { passive: false });
		bindings.listen(scrollbarRef, "wheel", handleWheel, { passive: false });

		// Scrollbar drag
		let scrollDragging = false;
		let scrollDragStartY = 0;
		let scrollDragStartOffset = 0;

		// Scrollbar track click: jump to position
		bindings.listen(scrollbarRef, "mousedown", (e: MouseEvent) => {
			if (e.target === scrollThumbRef) return; // thumb has its own handler
			if (!currentFrame || currentFrame.historySize === 0) return;
			e.preventDefault();
			// Cancel any in-flight/settling smooth gesture (scrollPosF non-null
			// suppresses normal repaints, and the wheel gesture-end timer would
			// otherwise re-settle over this jump) so the terminal_scroll jump below
			// actually repaints the view.
			resetSmoothScroll();
			const rect = scrollbarRef.getBoundingClientRect();
			const clickRatio = (e.clientY - rect.top) / rect.height;
			const targetOffset = Math.round((1 - clickRatio) * currentFrame.historySize);
			// Coalesced absolute jump (latest-wins, back-pressured) — same path as wheel/touch.
			scroll.pendingOffset = Math.max(0, Math.min(currentFrame.historySize, targetOffset));
			scheduleScrollFlush();
		});

		bindings.listen(scrollThumbRef, "mousedown", (e: MouseEvent) => {
			e.preventDefault();
			e.stopPropagation();
			// Cancel any in-flight/settling smooth gesture first; otherwise scrollPosF
			// stays non-null and scheduleRepaint bails, freezing the view while we drag
			// the thumb. (resetSmoothScroll also cancels the wheel gesture-end timer.)
			resetSmoothScroll();
			scrollDragging = true;
			scrollDragStartY = e.clientY;
			scrollDragStartOffset = currentFrame?.displayOffset ?? 0;
		});

		const onScrollDragMove = (e: MouseEvent) => {
			if (!scrollDragging || !currentFrame) return;
			const historySize = currentFrame.historySize;
			if (historySize === 0) return;
			// Use the cached track height (set in remeasure) instead of reading
			// scrollbarRef.clientHeight — a layout-forcing read on every mousemove.
			const trackHeight = scrollbarTrackHeight;
			const thumbHeight = parseFloat(scrollThumbRef.style.height) || 20;
			const scrollRange = trackHeight - thumbHeight;
			if (scrollRange <= 0) return;

			const dy = e.clientY - scrollDragStartY;
			const offsetDelta = Math.round((dy / scrollRange) * historySize);
			// Absolute target anchored to the drag start — NOT a delta vs the (async, often
			// stale) currentFrame.displayOffset, which would overshoot on fast drags. Routed
			// through the coalesced latest-wins flush so rapid mousemoves collapse to one IPC.
			scroll.pendingOffset = Math.max(0, Math.min(historySize, scrollDragStartOffset - offsetDelta));
			scheduleScrollFlush();
		};

		const onScrollDragUp = () => {
			scrollDragging = false;
		};

		bindings.listen(document, "mousemove", onScrollDragMove);
		bindings.listen(document, "mouseup", onScrollDragUp);

		// Widen the cleanup NOW, before the transport.subscribe() await below. If the
		// component unmounts mid-await, onCleanup runs while `unsubscribe` would
		// otherwise still cover only the transport — leaking all four document
		// listeners for the page lifetime.
		const detachDomListeners = () => {
			bindings.dispose();
			if (scrollRafId) cancelAnimationFrame(scrollRafId);
			if (selectionRafId !== undefined) cancelAnimationFrame(selectionRafId);
			resetSmoothScroll();
			stopSelectionScroll();
		};
		unsubscribe = () => {
			detachDomListeners();
			transport?.unsubscribe();
		};

		// Touch input (mobile/tablet)
		cleanupTouch = installTouchHandlers(canvasRef, touchTextareaRef, {
			onScrollPixels: (dy) => {
				// Touch is direct manipulation: the content must follow the finger,
				// the OPPOSITE of the wheel convention handleScrollDelta expects
				// (positive dy = toward newer/bottom). Negate so swipe-up reveals
				// newer lines and swipe-down reveals older scrollback, matching
				// native iOS scrolling.
				handleScrollDelta(-dy);
			},
			onScrollEnd: resetScrollGesture,
			onInput: (data) => writePty(data),
			onFocus: () => {
				setFocused(true);
				startBlink();
				props.onFocus?.();
			},
			onFontSizeChange: (delta) => {
				// Per-terminal, like every keyboard/menu/palette zoom: the global
				// default is persisted config, and the renderer reads the terminal's
				// own size first, so writing the default changed everything except
				// the terminal being pinched.
				applyPinchFontDelta(props.terminalId, delta, settingsStore.state.defaultFontSize);
			},
			onSelectionMode: () => {
				/* future: enter selection UI */
			},
		});

		// Subscribe to the grid channel. The transport and its session-event
		// listeners already exist — installed before the font load above.
		try {
			// Rust installs a fresh delivery gate on subscribe, so the echoed receipt
			// count has to start from zero with it.
			hiddenAck.cancel();
			framesReceived = 0;
			await transport.subscribe((data) => onFrame(data));
			if (!alive) {
				// Unmounted while subscribe() was in flight. onCleanup already ran and
				// tore down what existed then — but the grid subscription only became
				// live on the line above and would leak. Tear it down here.
				transport.unsubscribe();
				return;
			}
			// Paint the current grid now. The browser-mode WS subscribe (unlike the
			// Tauri event channel) does not replay the current frame, so an idle
			// session with no pending output would render nothing and leave the
			// canvas black until the first interaction. Forcing a full frame here is
			// idempotent on desktop and fixes the black-on-load in browser mode.
			noteFrameRequest();
			invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
		} catch (e) {
			appLogger.error("terminal", "Failed to subscribe to terminal grid channel", {
				sessionId: props.sessionId,
				error: e,
			});
			// `unsubscribe` already covers this on unmount; drop the session-event
			// listeners now rather than keeping them alive on a terminal that will
			// never paint.
			transport?.unsubscribe();
		}

		function scrollToBlock(direction: "previous" | "next") {
			const term = terminalsStore.get(props.terminalId);
			if (!term || !currentFrame) return;
			const blocks = term.commandBlocks;
			const active = term.activeBlock;
			const allPromptLines = blocks.map((b) => b.promptLine).concat(active ? [active.promptLine] : []);
			if (allPromptLines.length === 0) return;
			const currentViewLine = currentFrame.historySize - currentFrame.displayOffset;
			const targetLine = pickBlock(allPromptLines, currentViewLine, direction);
			if (targetLine === undefined) {
				// No further block ahead: land at the live tail, matching the old
				// behavior of falling back to scrollToBottom() on "next".
				if (direction === "next" && currentFrame.displayOffset > 0) {
					invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta: -currentFrame.displayOffset }).catch(
						ipcErr("terminal_scroll"),
					);
				}
				return;
			}
			invokeRef?.("terminal_scroll_to", { sessionId: props.sessionId, line: targetLine }).catch(
				ipcErr("terminal_scroll_to"),
			);
		}

		/** Shared by the gutter click's fold zone and the `Cmd+Shift+.` shortcut below —
		 *  toggle fold state for a specific, already-resolved block. */
		function toggleFoldForBlock(block: import("../../stores/terminals").CommandBlock) {
			if (!settingsStore.state.blockFoldingEnabled) return;
			const term = terminalsStore.get(props.terminalId);
			const alreadyFolded = term?.foldedBlocks.has(block.promptLine) ?? false;
			// Never fold ON a block with nothing to fold yet (still running, or a
			// degenerate empty block) — unfolding is always safe, but folding a
			// still-open block would silently pre-fold it before it ever appears in
			// commandBlocks, so it renders pre-folded the instant it closes with no
			// action from the user at that point.
			if (!canToggleFold(alreadyFolded, foldRange(block) !== null)) return;
			terminalsStore.toggleBlockFold(props.terminalId, block.promptLine);
			fullRepaintNeeded = true;
			const m = metrics();
			if (currentFrame && m) paintFrame(currentFrame, m);
		}

		function toggleBlockFoldAtViewport() {
			if (!settingsStore.state.blockFoldingEnabled) return;
			const term = terminalsStore.get(props.terminalId);
			if (!term || !currentFrame) return;
			const viewTop = currentFrame.historySize - currentFrame.displayOffset;
			const blocks = [...term.commandBlocks, term.activeBlock].filter(
				Boolean,
			) as import("../../stores/terminals").CommandBlock[];
			const current = findBlockAtViewport(blocks, viewTop, lastResizeRows >> 1);
			if (!current) return;
			toggleFoldForBlock(current);
		}

		props.onRef?.({
			focus: () => keyInputRef.focus({ preventScroll: true }),
			scrollToBlock,
			toggleBlockFoldAtViewport,
			getSelectionText: () => selection.cachedText,
			refresh: () => {
				rowMap.clear();
				clearDetectedLinks();
				fullRepaintNeeded = true;
				currentFrame = null;
				lastDisplayOffset = -1;
				lastResizeCols = 0;
				lastResizeRows = 0;
				remeasure();
				invokeRef?.("terminal_request_frame", { sessionId: props.sessionId }).catch(ipcErr("terminal_request_frame"));
			},
			resubscribe: async () => {
				// A pending hidden ack would report the old channel's receipt count
				// against the fresh gate Rust installs on resubscribe.
				hiddenAck.cancel();
				framesReceived = 0;
				await transport?.resubscribe();
			},
			searchFind: async (query: string, blockScope?: boolean) => {
				if (!query || !invokeRef) {
					clearSearchState();
					const m = metrics();
					if (currentFrame && m) paintFrame(currentFrame, m);
					return { index: -1, count: 0 };
				}
				// A genuinely new/different query must not inherit "active" from an old
				// match that coincidentally lands at the same row/col — only a re-run of
				// the SAME query (e.g. toggling block scope) should preserve it by value.
				const preserveActiveByValue = query === searchQuery;
				searchQuery = query;
				searchBlockScope = blockScope ?? false;
				return runSearchQuery(true, preserveActiveByValue);
			},
			searchNext: () => {
				const match = search.next();
				if (!match) return { index: -1, count: 0 };
				scrollToMatch(match);
				const m = metrics();
				if (currentFrame && m) paintFrame(currentFrame, m);
				return { index: search.activeIndex, count: search.matches.length };
			},
			searchPrev: () => {
				const match = search.previous();
				if (!match) return { index: -1, count: 0 };
				scrollToMatch(match);
				const m = metrics();
				if (currentFrame && m) paintFrame(currentFrame, m);
				return { index: search.activeIndex, count: search.matches.length };
			},
			searchClear: () => {
				clearSearchState();
				const m = metrics();
				if (currentFrame && m) paintFrame(currentFrame, m);
			},
			paste: (text: string) => {
				if (currentFrame?.bracketedPaste) {
					writePty(`\x1b[200~${text}\x1b[201~`);
				} else {
					writePty(text);
				}
			},
		});
	});

	// On-screen keyboard handling (touch only): slide THIS terminal up just enough
	// to reveal the CURSOR ROW above the virtual keyboard, without resizing the app
	// layout or the PTY (no reflow/SIGWINCH). The lift is anchored to the cursor —
	// NOT the pane's bottom edge — so a freshly-started agent whose input sits near
	// the TOP of the grid isn't shifted off-screen (the mirror of the original
	// occlusion bug). It's a pure transform on kbLiftRef (wraps the stage), clipped
	// by containerRef's overflow:hidden. Only the focused terminal lifts.
	function updateKeyboardLift() {
		const occ = keyboardOcclusion();
		const isFocused = focused();
		const m = metrics();
		if (!isTouchDevice || !kbLiftRef) return;
		const frame = currentFrame;
		// Clear when unfocused, keyboard closed, no frame/metrics yet, or scrolled
		// back into history (frame.displayOffset > 0 → cursor isn't on screen).
		if (!isFocused || occ <= 0 || !frame || !m || frame.displayOffset > 0) {
			kbLiftRef.style.transform = "";
			return;
		}
		// containerRef doesn't move (only its child kbLiftRef is transformed), so its
		// top is the stable viewport origin for the cursor's untransformed Y. Lift by
		// how far the cursor row's bottom sits below the keyboard's top edge — zero
		// when the cursor is already above the keyboard, so a top-anchored input never
		// gets pushed up. Anchoring to the cursor bottom self-clamps: the cursor can't
		// overshoot above containerRef.top.
		const keyboardTop = window.innerHeight - occ;
		const cursorBottomY = containerRef.getBoundingClientRect().top + (frame.cursorRow + 1) * m.cellHeight;
		const lift = Math.max(0, Math.round(cursorBottomY - keyboardTop));
		kbLiftRef.style.transform = lift > 0 ? `translateY(${-lift}px)` : "";
	}
	if (isTouchDevice) {
		ensureKeyboardViewportTracking();
		// React to keyboard open/close, focus, and metrics (font) changes. Cursor
		// movement is driven separately from the frame-decode path, since currentFrame
		// is a plain ref, not a signal.
		createEffect(() => {
			updateKeyboardLift();
		});
	}

	createEffect(() => {
		terminalsStore.state.terminals[props.terminalId]?.fontSize;
		settingsStore.state.defaultFontSize;
		settingsStore.state.font;
		settingsStore.state.fontWeight;
		if (!alive) return;
		settingsStore.state.theme;
		invalidateGlyphCache();
		gridRenderer?.invalidateCaches();
		fullRepaintNeeded = true;
		remeasure();
	});

	// Force an immediate scrollbar-marks repaint on toggle — paintScrollbarMarks
	// is otherwise only invoked from the frame-decode/scroll path, so without
	// this, flipping a setting wouldn't take visible effect until the next
	// unrelated repaint. If a future mark type gets its own setting, it needs
	// tracking here too, alongside its own placeholder branch in
	// paintScrollbarMarks's memo key below — the two must stay in sync.
	createEffect(() => {
		settingsStore.state.showBlockMarks;
		settingsStore.state.showPromptMarks;
		if (!alive || !currentFrame) return;
		updateScrollbar(currentFrame);
	});

	// Same reasoning as the scrollbar-marks effect above, for the gutter-overlay
	// settings: paintBlockTimestamps/paintFoldChevrons are only invoked from the
	// frame-decode/scroll path, so toggling the timestamp mode or block folding
	// on an otherwise-idle terminal wouldn't take visible effect until the next
	// unrelated repaint.
	createEffect(() => {
		settingsStore.state.blockTimestampMode;
		settingsStore.state.blockFoldingEnabled;
		if (!alive || !currentFrame) return;
		const m = metrics();
		if (m) repaintOverlay(currentFrame, m);
	});

	// React to link-activation mode changes and (in "modifier" mode) to the
	// Cmd/Ctrl hold itself — see linkModifierEffectDecision. Link underlines
	// are overlay-only, so a repaint suffices; no fullRepaintNeeded.
	createEffect(() => {
		const mode = settingsStore.state.linkActivation;
		const held = linkModifierHeld();
		if (!alive) return;
		const decision = linkModifierEffectDecision(mode, held, lastLinkHoverEvent !== null);
		if (decision.clearHover) {
			hoveredLink = null;
			canvasRef.style.cursor = "text";
		}
		if (currentFrame) {
			const m = metrics();
			if (m) repaintOverlay(currentFrame, m);
		}
		if (decision.recheckHover && lastLinkHoverEvent) {
			const pos = canvasToGrid(lastLinkHoverEvent);
			checkLinksAtRow(pos.row, pos.col);
		}
	});

	async function copySelection() {
		const setStatus = (window as unknown as Record<string, unknown>).__tuic_setStatusInfo as
			| ((msg: string) => void)
			| undefined;
		try {
			let text: string;
			// Always prefer the Rust path: it unwraps soft-wrapped logical lines via the
			// WRAPLINE flag (grid_get_selection_text), so copying a line the terminal merely
			// wrapped for width doesn't insert a spurious newline. The JS fallback below has
			// no wrap info (see getLocalSelectionText DEFERRED) and only runs when invoke or
			// the selection coords are unavailable.
			if (invokeRef && selection.start && selection.end) {
				text = (await invokeRef("terminal_get_selection_text", {
					sessionId: props.sessionId,
					startRow: selection.start.row,
					startCol: selection.start.col,
					endRow: selection.end.row,
					endCol: selection.end.col,
				})) as string;
				// Fall back to the local read if the IPC path yields nothing (transient error,
				// grid not ready). Loses wrap-unwrapping, but a wrapped copy beats a silent
				// no-op — the onscreen path could always satisfy a copy before this routing.
				if (!text) text = getLocalSelectionText();
			} else {
				text = getLocalSelectionText();
			}
			if (text) {
				selection.cachedText = text;
				await writeClipboard(text);
				setStatus?.("Copied to clipboard");
			}
		} catch (e) {
			appLogger.warn("terminal", "Clipboard write failed", { error: e });
			setStatus?.("Copy failed — clipboard unavailable");
		}
	}

	onCleanup(() => {
		alive = false;
		stopBlink();
		if (rafId !== undefined) {
			cancelAnimationFrame(rafId);
			rafId = undefined;
		}
		// Smooth-scroll RAF + settle timer also outlive unmount and run against the
		// released row cache — cancel both.
		if (smoothRafId) {
			cancelAnimationFrame(smoothRafId);
			smoothRafId = 0;
		}
		clearSettlePending();
		hiddenAck.cancel();
		if (reconcileTimer) clearTimeout(reconcileTimer);
		clearTimeout(resizeDebounce);
		resizeObserver?.disconnect();
		visibilityObserver?.disconnect();
		if (dprChangeHandler) dprMediaQuery?.removeEventListener("change", dprChangeHandler);
		unsubscribe?.();
		cleanupTouch?.();
		clearTimeout(linkThrottle);
		clearTimeout(scrollGestureEndTimer);
		searchRefresh.cancel();
		linkController.dispose();
		resetFrameTiming(props.sessionId);
		rowMap.clear();
		gridRenderer?.invalidateCaches();
		releaseCache();
	});

	return (
		<div
			ref={containerRef!}
			data-terminal-container
			style={{
				position: "relative",
				width: "100%",
				height: "100%",
				overflow: "hidden",
			}}
			onDragOver={(e) => {
				if (e.dataTransfer?.types?.includes("application/x-tuic-path")) {
					e.preventDefault();
					e.dataTransfer.dropEffect = "copy";
				}
			}}
			onDrop={(e) => {
				const path = e.dataTransfer?.getData("application/x-tuic-path");
				if (!path) return;
				e.preventDefault();
				const quoted = `'${path.replace(/'/g, "'\\''")}' `;
				writePty(quoted);
				keyInputRef.focus({ preventScroll: true });
			}}
		>
			{/* Offscreen textarea for mobile virtual keyboard input */}
			<textarea
				ref={touchTextareaRef!}
				style={{
					position: "fixed",
					top: "-9999px",
					left: "-9999px",
					width: "1px",
					height: "1px",
					opacity: "0",
					"pointer-events": "none",
				}}
				autocomplete="off"
				autocorrect="off"
				autocapitalize="off"
				spellcheck={false}
				tabIndex={-1}
			/>
			{/* Hidden input that receives all keyboard events including dead-key composition.
			    Canvas elements in WKWebView don't participate in the macOS text input system,
			    so dead keys (quotes, accents, etc.) are lost when listeners live on the canvas.
			    Using a real <input> fixes composition on macOS without affecting rendering. */}
			<input
				ref={keyInputRef!}
				type="text"
				aria-hidden="true"
				style={{
					position: "absolute",
					top: "0",
					left: "0",
					width: "1px",
					height: "1em",
					opacity: "0",
					border: "none",
					outline: "none",
					padding: "0",
					margin: "0",
					overflow: "hidden",
					"pointer-events": "none",
					"font-size": "1px",
					"z-index": "-1",
				}}
				tabIndex={-1}
				autocomplete="off"
				autocorrect="off"
				autocapitalize="off"
				spellcheck={false}
			/>
			{/* Keyboard-lift wrapper: slides the whole stage up on touch devices so the
			    cursor stays above the on-screen keyboard. Identity transform at rest;
			    clipped by containerRef's overflow:hidden. */}
			<div
				ref={kbLiftRef!}
				style={{
					position: "absolute",
					inset: "0",
					"will-change": "transform",
				}}
			>
				{/* Smooth-scroll stage: base + overlay translate together during a gesture.
				    At rest transform is identity → geometry/coordinates are unchanged. */}
				<div
					ref={stageRef!}
					style={{
						position: "absolute",
						top: "0",
						left: "0",
						"will-change": "transform",
					}}
				>
					{/* Overscan: the row above/below the viewport, revealed as the stage slides.
				    Sits behind the (opaque) base canvas; never hit-tested. */}
					<canvas
						ref={overscanCanvasRef!}
						style={{
							position: "absolute",
							left: "0",
							"pointer-events": "none",
						}}
					/>
					<canvas
						ref={canvasRef!}
						style={{
							position: "relative",
							display: "block",
							outline: "none",
							cursor: "text",
						}}
						tabIndex={0}
					/>
					{/* Overlay canvas: cursor, selection, search highlights — redrawn every frame without touching base canvas */}
					<canvas
						ref={overlayCanvasRef!}
						style={{
							position: "absolute",
							top: "0",
							left: "0",
							"pointer-events": "none",
						}}
					/>
					{/* Suggest/intent overlay — inside the stage so it scrolls with the content
				    (rebuilt from the row cache during a smooth-scroll gesture). */}
					<div
						ref={overlayRef!}
						style={{
							position: "absolute",
							top: "0",
							left: "0",
							right: "0",
							bottom: "0",
							"pointer-events": "none",
							"z-index": "10",
							overflow: "hidden",
						}}
					/>
				</div>
			</div>
			{/* Scrollbar */}
			<div
				ref={scrollbarRef!}
				data-testid="terminal-scrollbar"
				style={{
					position: "absolute",
					top: "0",
					right: "0",
					width: "14px",
					height: "100%",
					display: "none",
					"z-index": "20",
				}}
			>
				<div
					ref={scrollThumbRef!}
					data-testid="terminal-scrollbar-thumb"
					onMouseEnter={(e) => {
						// Darker, subtle hover like the old terminal scrollbar (#cccccc @0.3),
						// not the bright --fg-muted.
						e.currentTarget.style.background = "rgba(204, 204, 204, 0.3)";
					}}
					onMouseLeave={(e) => {
						e.currentTarget.style.background = "var(--bg-highlight)";
					}}
					style={{
						width: "10px",
						"margin-left": "2px",
						"border-radius": "5px",
						// Harmonized with the editor scrollbar: same --bg-highlight resting
						// color, --fg-muted on hover, and a hand pointer cursor.
						background: "var(--bg-highlight)",
						"min-height": "20px",
						position: "absolute",
						top: "0",
						cursor: "pointer",
					}}
				/>
			</div>
			<ContextMenu
				items={[
					// Link detection takes priority when both apply to the same
					// click (e.g. a URL is both a detected link AND matches the
					// built-in iterm-http-url rule) — its Open/Copy-link pair
					// already covers that span, so showing the rule's own
					// Open/Copy actions too would just duplicate the label.
					...(() => {
						const match = smartMenuMatch();
						if (!match || linkMenuTarget()) return [];
						const ruleName = match.rule.name.trim();
						return [
							// Non-actionable header naming which rule matched, so the
							// actions below aren't a mystery list. Skipped for a
							// user-added rule left with a blank (or whitespace-only) name.
							...(ruleName ? [{ label: ruleName, header: true, action: () => {} }] : []),
							...match.rule.actions.map((action) => ({
								label: action.title,
								action: () => runSmartAction(action, match),
							})),
						];
					})(),
					...(linkMenuTarget()
						? [
								{
									label: "Open",
									action: () => {
										const t = linkMenuTarget();
										if (t) openLink(t);
									},
								},
								{
									label: "Copy link",
									action: () => {
										const t = linkMenuTarget();
										if (t) copyLink(t);
									},
								},
							]
						: []),
				]}
				x={linkMenu.position().x}
				y={linkMenu.position().y}
				visible={linkMenu.visible()}
				onClose={() => {
					linkMenu.close();
					setSmartMenuMatch(null);
				}}
			/>
		</div>
	);
};

export default CanvasTerminal;
