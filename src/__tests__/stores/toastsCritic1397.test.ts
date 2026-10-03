import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope } from "../helpers/store";

describe("toastsStore critic 1397", () => {
	let toastsStore: typeof import("../../stores/toasts").toastsStore;
	let activityStore: typeof import("../../stores/activityStore").activityStore;

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		toastsStore = (await import("../../stores/toasts")).toastsStore;
		activityStore = (await import("../../stores/activityStore")).activityStore;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	const reachable = (title: string) =>
		toastsStore.toasts.some((t) => t.title === title) || activityStore.getActive().some((i) => i.title === title);

	it("a bell-opted-out toast raised while two cards are visible is not silently lost", () => {
		// catches: overflow path drops a mirrorInBell=false toast (Progress outcome) — neither card nor bell item
		testInScope(() => {
			toastsStore.add("A", "", "info", false, undefined, 0);
			toastsStore.add("B", "", "info", false, undefined, 0);
			toastsStore.add("Progress", "Recorded", "info", false, undefined, undefined, "/repo", undefined, false);
			expect(reachable("Progress")).toBe(true);
		});
	});

	it("two identical backend notices in a row leave one bell item", () => {
		// catches: removal of the hasVisible dedup — a duplicated mcp-toast event stacks copies in the bell
		testInScope(() => {
			toastsStore.addToBell("Dup", "same", "info", "/repo");
			toastsStore.addToBell("Dup", "same", "info", "/repo");
			expect(activityStore.getActive().filter((i) => i.title === "Dup")).toHaveLength(1);
		});
	});

	it("a same-kind toast exactly 5000 ms after the first joins its group", () => {
		// catches: off-by-one in the 5 s coalescing window (< instead of <=)
		testInScope(() => {
			toastsStore.add("Saved", "one", "info", false, undefined, 0);
			toastsStore.add("Other", "x", "info", false, undefined, 0);
			vi.advanceTimersByTime(5000);
			toastsStore.add("Saved", "two", "info", false, undefined, 0);
			expect(toastsStore.toasts).toHaveLength(2);
			expect(toastsStore.toasts.find((t) => t.title === "Saved")?.count).toBe(2);
		});
	});

	it("a grouped toast that is dismissed starts a fresh count next time", () => {
		// catches: count surviving removal, so the next Saved shows ×3 instead of a plain card
		testInScope(() => {
			const id = toastsStore.add("Saved", "one", "info", false, undefined, 0);
			toastsStore.add("Saved", "two", "info", false, undefined, 0);
			toastsStore.remove(id);
			toastsStore.add("Saved", "three", "info", false, undefined, 0);
			expect(toastsStore.toasts).toHaveLength(1);
			expect(toastsStore.toasts[0].count).toBeUndefined();
		});
	});
});
