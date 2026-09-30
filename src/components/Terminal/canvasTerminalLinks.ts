export interface LinkSpan {
	colStart: number;
	colEnd: number;
}

export function isOverSpan(spans: readonly LinkSpan[] | undefined, col: number): boolean {
	return spans?.some((sp) => col >= sp.colStart && col < sp.colEnd) ?? false;
}

/**
 * Whether a press under mouse reporting belongs to the underlined link beneath
 * the pointer instead of the app. The underline promises that a click opens the
 * link, so the left and right buttons must not be forwarded there; the middle
 * button stays the app's.
 */
export function linkClaimsPress(button: number, overLink: boolean): boolean {
	return overLink && (button === 0 || button === 2);
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
