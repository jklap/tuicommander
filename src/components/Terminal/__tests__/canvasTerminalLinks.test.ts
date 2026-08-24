import { afterEach, describe, expect, it, vi } from "vitest";
import {
	createCanvasLinkController,
	linkModifierEffectDecision,
	linkVisuals,
	shouldOpenOnClick,
	shouldResolveLinkHoverOnMove,
	shouldSkipMouseReportForLink,
} from "../canvasTerminalLinks";

afterEach(() => vi.useRealTimers());

describe("linkVisuals", () => {
	it("click mode always paints dashed/solid underlines and the pointer cursor", () => {
		expect(linkVisuals("click", false)).toEqual({ dashed: true, solid: true, pointer: true });
		expect(linkVisuals("click", true)).toEqual({ dashed: true, solid: true, pointer: true });
	});

	it("modifier mode paints nothing until the modifier is held", () => {
		expect(linkVisuals("modifier", false)).toEqual({ dashed: false, solid: false, pointer: false });
		expect(linkVisuals("modifier", true)).toEqual({ dashed: true, solid: true, pointer: true });
	});

	it("never mode keeps the dashed underline but never hover-highlights", () => {
		expect(linkVisuals("never", false)).toEqual({ dashed: true, solid: false, pointer: false });
		expect(linkVisuals("never", true)).toEqual({ dashed: true, solid: false, pointer: false });
	});
});

describe("shouldOpenOnClick", () => {
	it("click mode always opens", () => {
		expect(shouldOpenOnClick("click", false)).toBe(true);
		expect(shouldOpenOnClick("click", true)).toBe(true);
	});

	it("modifier mode opens only when the modifier was held on the click event", () => {
		expect(shouldOpenOnClick("modifier", false)).toBe(false);
		expect(shouldOpenOnClick("modifier", true)).toBe(true);
	});

	it("never mode never opens on click regardless of modifier", () => {
		expect(shouldOpenOnClick("never", false)).toBe(false);
		expect(shouldOpenOnClick("never", true)).toBe(false);
	});
});

describe("shouldSkipMouseReportForLink", () => {
	it("never withholds when the press isn't over a link", () => {
		for (const mode of ["click", "modifier", "never"] as const) {
			expect(shouldSkipMouseReportForLink(mode, 0, true, false)).toBe(false);
			expect(shouldSkipMouseReportForLink(mode, 2, true, false)).toBe(false);
		}
	});

	it("always withholds a right-click on a link, regardless of mode or modifier (#57)", () => {
		for (const mode of ["click", "modifier", "never"] as const) {
			expect(shouldSkipMouseReportForLink(mode, 2, false, true)).toBe(true);
			expect(shouldSkipMouseReportForLink(mode, 2, true, true)).toBe(true);
		}
	});

	it("withholds a modifier+left-click on a link only in modifier mode while held", () => {
		expect(shouldSkipMouseReportForLink("modifier", 0, true, true)).toBe(true);
		expect(shouldSkipMouseReportForLink("modifier", 0, false, true)).toBe(false);
	});

	it("never withholds a plain left-click on a link in click mode — it opens via the click event, not by skipping the forward", () => {
		expect(shouldSkipMouseReportForLink("click", 0, false, true)).toBe(false);
		expect(shouldSkipMouseReportForLink("click", 0, true, true)).toBe(false);
	});

	it("never withholds a left-click on a link in never mode", () => {
		expect(shouldSkipMouseReportForLink("never", 0, false, true)).toBe(false);
		expect(shouldSkipMouseReportForLink("never", 0, true, true)).toBe(false);
	});

	it("does not withhold a middle-click (button 1) on a link", () => {
		expect(shouldSkipMouseReportForLink("modifier", 1, true, true)).toBe(false);
	});
});

describe("shouldResolveLinkHoverOnMove", () => {
	it("always resolves in click mode regardless of modifier", () => {
		expect(shouldResolveLinkHoverOnMove("click", false)).toBe(true);
		expect(shouldResolveLinkHoverOnMove("click", true)).toBe(true);
	});

	it("always resolves in never mode regardless of modifier", () => {
		expect(shouldResolveLinkHoverOnMove("never", false)).toBe(true);
		expect(shouldResolveLinkHoverOnMove("never", true)).toBe(true);
	});

	it("resolves in modifier mode only while the modifier is held", () => {
		expect(shouldResolveLinkHoverOnMove("modifier", false)).toBe(false);
		expect(shouldResolveLinkHoverOnMove("modifier", true)).toBe(true);
	});
});

describe("linkModifierEffectDecision", () => {
	it("does nothing in click mode regardless of modifier or a pending hover event", () => {
		for (const held of [false, true]) {
			for (const hasHoverEvent of [false, true]) {
				expect(linkModifierEffectDecision("click", held, hasHoverEvent)).toEqual({
					clearHover: false,
					recheckHover: false,
				});
			}
		}
	});

	it("does nothing in never mode regardless of modifier or a pending hover event", () => {
		for (const held of [false, true]) {
			for (const hasHoverEvent of [false, true]) {
				expect(linkModifierEffectDecision("never", held, hasHoverEvent)).toEqual({
					clearHover: false,
					recheckHover: false,
				});
			}
		}
	});

	it("clears hover on modifier release in modifier mode", () => {
		expect(linkModifierEffectDecision("modifier", false, false)).toEqual({
			clearHover: true,
			recheckHover: false,
		});
		expect(linkModifierEffectDecision("modifier", false, true)).toEqual({
			clearHover: true,
			recheckHover: false,
		});
	});

	it("rechecks hover on modifier press in modifier mode only if a hover position was recorded", () => {
		expect(linkModifierEffectDecision("modifier", true, true)).toEqual({
			clearHover: false,
			recheckHover: true,
		});
		expect(linkModifierEffectDecision("modifier", true, false)).toEqual({
			clearHover: false,
			recheckHover: false,
		});
	});
});

describe("canvas terminal link controller", () => {
	it("invalidates older hover checks and every check after disposal", () => {
		const links = createCanvasLinkController();
		const first = links.beginCheck();
		const second = links.beginCheck();
		expect(links.isCurrent(first)).toBe(false);
		expect(links.isCurrent(second)).toBe(true);

		links.dispose();
		expect(links.isCurrent(second)).toBe(false);
	});

	it("coalesces verification and cancels a queued run on disposal", async () => {
		vi.useFakeTimers();
		const verify = vi.fn();
		const links = createCanvasLinkController(25);
		links.scheduleVerification(verify);
		links.scheduleVerification(verify);
		await vi.advanceTimersByTimeAsync(25);
		expect(verify).toHaveBeenCalledTimes(1);

		links.scheduleVerification(verify);
		links.dispose();
		await vi.runAllTimersAsync();
		expect(verify).toHaveBeenCalledTimes(1);
	});

	it("owns and clears discovery caches", () => {
		const links = createCanvasLinkController();
		links.rowCache.set("row", []);
		links.fileCache.set("file", { spans: [{ colStart: 1, colEnd: 2 }], ts: 1 });
		links.detectedSpans.set(2, [{ colStart: 3, colEnd: 4 }]);
		links.wrappedSpans.set(3, [{ colStart: 0, colEnd: 5 }]);

		links.clearDetected();
		expect(links.detectedSpans.size).toBe(0);
		expect(links.wrappedSpans.size).toBe(0);
		expect(links.rowCache.size).toBe(1);
		expect(links.fileCache.size).toBe(1);
	});

	it("invalidates active checks and queued verification when detected rows are replaced", async () => {
		vi.useFakeTimers();
		const verify = vi.fn();
		const links = createCanvasLinkController(25);
		const oldGeneration = links.beginCheck();
		links.scheduleVerification(verify);

		links.clearDetected();

		expect(links.isCurrent(oldGeneration)).toBe(false);
		await vi.runAllTimersAsync();
		expect(verify).not.toHaveBeenCalled();
	});
});
