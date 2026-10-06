/**
 * Wiring coverage for the frame-starvation watchdog (the pure decision logic is
 * in `frameStarvationWatchdog.test.ts`; this proves `CanvasTerminal` actually
 * uses it).
 *
 * The bug: a visible terminal whose grid channel had silently died — Rust still
 * sent frames, but into a callback the webview no longer had — stayed blank
 * forever, because nothing on either side could tell. These tests mount the real
 * component and check each of the ways the wiring can go wrong: not armed on the
 * initial subscribe, not armed on show, armed while hidden, left running after
 * unmount, or the reattach `resubscribe` diverging from the one the watchdog uses.
 *
 * The watchdog's timeout is shrunk to 150 ms through the module mock below, so the
 * tests run on real timers (fake timers deadlock `mountCanvasTerminal`'s
 * `waitFor`). Each "does NOT resubscribe" test waits several multiples of that.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { appLogger } from "../../../stores/appLogger";
import { buildTextFrame } from "./helpers/frameFixture";
import {
	createFakeTransport,
	FIXED_CELL_METRICS,
	mountCanvasTerminal,
	stubCanvasEnvironment,
} from "./helpers/mountCanvasTerminal";

const fakeTransport = vi.hoisted(() => ({ current: null as ReturnType<typeof createFakeTransport> | null }));

vi.mock("../canvasTerminalTransport", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../canvasTerminalTransport")>();
	return { ...actual, createTransport: () => fakeTransport.current! };
});

vi.mock("../glyphCache", () => ({
	getSharedMetrics: () => FIXED_CELL_METRICS,
	acquireCache: vi.fn(),
	releaseCache: vi.fn(),
	invalidateGlyphCache: vi.fn(),
}));

// Long enough to outlast mountCanvasTerminal's waitFor poll (50 ms) plus dispose()'s rAF, so a
// timer armed by the initial subscribe is still pending when a test unmounts or hides.
const TEST_TIMEOUT_MS = 150;

vi.mock("../canvasTerminalUtils", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../canvasTerminalUtils")>();
	return {
		...actual,
		createFrameStarvationWatchdog: (opts: Parameters<typeof actual.createFrameStarvationWatchdog>[0]) =>
			actual.createFrameStarvationWatchdog({ ...opts, timeoutMs: TEST_TIMEOUT_MS }),
	};
});

/** Controllable IntersectionObserver: the component's callback is captured so a
 *  test can drive the tab hidden/visible exactly as the browser would. */
const observers: { cb: IntersectionObserverCallback; el: Element }[] = [];

class ControllableIO {
	constructor(private cb: IntersectionObserverCallback) {}
	observe(el: Element) {
		observers.push({ cb: this.cb, el });
	}
	unobserve() {}
	disconnect() {}
	takeRecords() {
		return [];
	}
}

function setVisible(isIntersecting: boolean) {
	for (const o of observers) {
		o.cb([{ isIntersecting, target: o.el } as IntersectionObserverEntry], {} as IntersectionObserver);
	}
}

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
/** Several watchdog periods: long enough that a timer which was going to fire has. */
const SETTLE = TEST_TIMEOUT_MS * 4;

describe("CanvasTerminal frame-starvation watchdog wiring", () => {
	let restoreEnv: () => void;
	let resubscribe: ReturnType<typeof vi.fn<() => Promise<void>>>;

	beforeEach(() => {
		observers.length = 0;
		vi.stubGlobal("IntersectionObserver", ControllableIO);
		restoreEnv = stubCanvasEnvironment();
		fakeTransport.current = createFakeTransport();
		resubscribe = vi.fn<() => Promise<void>>(async () => {});
		fakeTransport.current.resubscribe = resubscribe;
	});

	afterEach(() => {
		restoreEnv();
		vi.unstubAllGlobals();
	});

	it("resubscribes a visible terminal that gets no frame after its initial frame request", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s1", terminalId: "t1" });

		await sleep(SETTLE);

		expect(resubscribe).toHaveBeenCalled();
		await mounted.dispose();
	});

	it("leaves a healthy terminal alone when frames arrive", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s2", terminalId: "t2" });
		fakeTransport.current!.pushFrame(buildTextFrame(["hello"], 40));

		await sleep(SETTLE);

		expect(resubscribe).not.toHaveBeenCalled();
		await mounted.dispose();
	});

	it("does not run while the terminal is hidden", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s3", terminalId: "t3" });
		setVisible(false); // cancels the check armed by the initial subscribe

		await sleep(SETTLE);

		expect(resubscribe).not.toHaveBeenCalled();
		await mounted.dispose();
	});

	it("re-arms on show, so a terminal that becomes visible onto a dead channel heals", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s4", terminalId: "t4" });
		fakeTransport.current!.pushFrame(buildTextFrame(["hello"], 40));
		setVisible(false);
		await sleep(SETTLE);
		expect(resubscribe).not.toHaveBeenCalled();

		setVisible(true); // the show path: remeasure + terminal_request_frame, then arm
		await sleep(SETTLE);

		expect(resubscribe).toHaveBeenCalled();
		expect(fakeTransport.current!.invokeCalls.some((c) => c.cmd === "terminal_request_frame")).toBe(true);
		await mounted.dispose();
	});

	it("does not resubscribe after a show when the requested frame does arrive", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s5", terminalId: "t5" });
		fakeTransport.current!.pushFrame(buildTextFrame(["hello"], 40));
		setVisible(false);
		setVisible(true);
		fakeTransport.current!.pushFrame(buildTextFrame(["hello again"], 40));

		await sleep(SETTLE);

		expect(resubscribe).not.toHaveBeenCalled();
		await mounted.dispose();
	});

	it("gives up after a bounded number of attempts instead of looping", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s6", terminalId: "t6" });

		await sleep(TEST_TIMEOUT_MS * 12);

		expect(resubscribe).toHaveBeenCalledTimes(3);
		await mounted.dispose();
	});

	it("stops retrying once a resubscribe brings frames back", async () => {
		// Frames arrive over IPC after the resubscribe call returns, never inside it.
		resubscribe.mockImplementation(async () => {
			setTimeout(() => fakeTransport.current?.pushFrame(buildTextFrame(["recovered"], 40)), 10);
		});
		const mounted = await mountCanvasTerminal({ sessionId: "s7", terminalId: "t7" });

		await sleep(TEST_TIMEOUT_MS * 8);

		expect(resubscribe).toHaveBeenCalledTimes(1);
		await mounted.dispose();
	});

	it("is cancelled on unmount — no resubscribe on a disposed component", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s8", terminalId: "t8" });
		await mounted.dispose();

		await sleep(SETTLE);

		expect(resubscribe).not.toHaveBeenCalled();
	});

	// The `alive`/`hidden` guard inside onStarved already stops a stray check from
	// resubscribing, so a missing cancel() is invisible in `resubscribe` calls. It is
	// not invisible in the log: the orphaned watchdog keeps re-arming until it
	// exhausts its attempts and reports "giving up" for a terminal nobody is looking at.
	describe("a watchdog that should be stopped is actually cancelled", () => {
		const givingUp = () =>
			(appLogger.error as unknown as ReturnType<typeof vi.fn>).mock.calls.filter((c) =>
				String(c[1]).includes("giving up"),
			);

		beforeEach(() => {
			vi.spyOn(appLogger, "error");
			vi.spyOn(appLogger, "warn");
		});

		afterEach(() => {
			vi.restoreAllMocks();
		});

		it("hiding the tab stops the retry loop, not just the resubscribe", async () => {
			const mounted = await mountCanvasTerminal({ sessionId: "s11", terminalId: "t11" });
			setVisible(false);

			await sleep(TEST_TIMEOUT_MS * 8);

			expect(givingUp()).toHaveLength(0);
			await mounted.dispose();
		});

		it("unmounting stops the retry loop, not just the resubscribe", async () => {
			const mounted = await mountCanvasTerminal({ sessionId: "s12", terminalId: "t12" });
			await mounted.dispose();

			await sleep(TEST_TIMEOUT_MS * 8);

			expect(givingUp()).toHaveLength(0);
		});
	});

	it("arms on the delayed show path too, when layout has not produced a box yet", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "s13", terminalId: "t13" });
		fakeTransport.current!.pushFrame(buildTextFrame(["hello"], 40));
		setVisible(false);
		await sleep(SETTLE);
		expect(resubscribe).not.toHaveBeenCalled();

		// display:none -> display:block: the box is still 0x0 when the observer fires,
		// so the show path defers its remeasure + frame request to the next frame.
		const fixedRect = Element.prototype.getBoundingClientRect;
		Element.prototype.getBoundingClientRect = () =>
			({ left: 0, top: 0, width: 0, height: 0, right: 0, bottom: 0, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect;
		try {
			setVisible(true);
		} finally {
			Element.prototype.getBoundingClientRect = fixedRect;
		}
		await sleep(SETTLE);

		expect(resubscribe).toHaveBeenCalled();
		await mounted.dispose();
	});

	describe("the reattach path shares the watchdog's resubscribe", () => {
		it("onRef.resubscribe rebuilds the channel", async () => {
			const mounted = await mountCanvasTerminal({ sessionId: "s9", terminalId: "t9" });
			fakeTransport.current!.pushFrame(buildTextFrame(["hello"], 40));

			await mounted.ref.resubscribe();

			expect(resubscribe).toHaveBeenCalledTimes(1);
			await mounted.dispose();
		});

		it("restarts the receipt count at zero, matching the fresh backend gate", async () => {
			const mounted = await mountCanvasTerminal({ sessionId: "s10", terminalId: "t10" });
			const t = fakeTransport.current!;
			t.pushFrame(buildTextFrame(["a"], 40));
			t.pushFrame(buildTextFrame(["b"], 40));
			t.pushFrame(buildTextFrame(["c"], 40));
			expect(t.ackCalls.at(-1)).toBe(3);

			await mounted.ref.resubscribe();
			t.pushFrame(buildTextFrame(["d"], 40));

			// A count that kept running (4) would be credited against a gate that has
			// sent one frame — opening it for frames the terminal never received.
			expect(t.ackCalls.at(-1)).toBe(1);
			await mounted.dispose();
		});
	});
});
