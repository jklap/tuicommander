import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * `keyboardViewport.ts` had zero test coverage (found while investigating an
 * unrelated "bottom of terminal not visible" report — this mechanism turned
 * out not to be the cause there, but the gap is real on its own: a regression
 * here would silently leave the on-screen-keyboard cursor lift broken with
 * nothing in `make check` to catch it).
 *
 * The module keeps `installed` and the `keyboardOcclusion` signal at module
 * scope (a deliberate process-global singleton — see its own doc comment), so
 * every test needs a FRESH module instance via `vi.resetModules()` + a
 * dynamic `import()`, not the top-level import — otherwise the second test to
 * call `ensureKeyboardViewportTracking()` would hit the `if (installed)
 * return` short-circuit from the first test's install and prove nothing.
 */

type FakeVisualViewport = {
	offsetTop: number;
	height: number;
	addEventListener: (type: string, listener: () => void) => void;
	removeEventListener: (type: string, listener: () => void) => void;
	listenerCounts: Record<string, number>;
	fire: (type: "resize" | "scroll") => void;
};

function makeFakeVisualViewport(offsetTop: number, height: number): FakeVisualViewport {
	const listeners: Record<string, Array<() => void>> = { resize: [], scroll: [] };
	return {
		offsetTop,
		height,
		listenerCounts: { resize: 0, scroll: 0 },
		addEventListener(type, listener) {
			listeners[type] ??= [];
			listeners[type].push(listener);
			this.listenerCounts[type] = (this.listenerCounts[type] ?? 0) + 1;
		},
		removeEventListener(type, listener) {
			listeners[type] = (listeners[type] ?? []).filter((l) => l !== listener);
		},
		fire(type) {
			for (const l of listeners[type] ?? []) l();
		},
	};
}

async function freshModule() {
	vi.resetModules();
	return import("../../utils/keyboardViewport");
}

describe("keyboardViewport", () => {
	beforeEach(() => {
		// The module reads `requestAnimationFrame` synchronously via its own
		// debounce — run callbacks immediately so `update()`'s effect is
		// observable without an extra microtask/frame flush in every test.
		vi.stubGlobal("requestAnimationFrame", (cb: () => void) => {
			cb();
			return 1;
		});
		vi.stubGlobal("cancelAnimationFrame", () => {});
		Object.defineProperty(window, "innerHeight", { value: 800, writable: true, configurable: true });
	});

	afterEach(() => {
		vi.unstubAllGlobals();
		Object.defineProperty(window, "visualViewport", { value: undefined, writable: true, configurable: true });
		Object.defineProperty(window, "innerHeight", { value: 768, writable: true, configurable: true });
	});

	it("stays 0 with no VisualViewport (desktop) and does not throw", async () => {
		Object.defineProperty(window, "visualViewport", { value: undefined, writable: true, configurable: true });
		const { keyboardOcclusion, ensureKeyboardViewportTracking } = await freshModule();

		expect(() => ensureKeyboardViewportTracking()).not.toThrow();
		expect(keyboardOcclusion()).toBe(0);
	});

	it("computes occlusion as innerHeight minus the visual viewport's covered band", async () => {
		// innerHeight=800, visualViewport covers offsetTop=0..height=500 —
		// a keyboard occluding the bottom 300px.
		const vv = makeFakeVisualViewport(0, 500);
		Object.defineProperty(window, "visualViewport", { value: vv, writable: true, configurable: true });
		const { keyboardOcclusion, ensureKeyboardViewportTracking } = await freshModule();

		ensureKeyboardViewportTracking();

		expect(keyboardOcclusion()).toBe(300);
	});

	it("clamps to 0 rather than going negative when nothing is occluded", async () => {
		// visualViewport covers the full layout viewport (no keyboard) —
		// innerHeight - (offsetTop + height) would be 0 here, and must never
		// read negative even if a platform reports height > innerHeight.
		const vv = makeFakeVisualViewport(0, 850);
		Object.defineProperty(window, "visualViewport", { value: vv, writable: true, configurable: true });
		const { keyboardOcclusion, ensureKeyboardViewportTracking } = await freshModule();

		ensureKeyboardViewportTracking();

		expect(keyboardOcclusion()).toBe(0);
	});

	it("recomputes on a visualViewport resize (keyboard opening further)", async () => {
		const vv = makeFakeVisualViewport(0, 500);
		Object.defineProperty(window, "visualViewport", { value: vv, writable: true, configurable: true });
		const { keyboardOcclusion, ensureKeyboardViewportTracking } = await freshModule();
		ensureKeyboardViewportTracking();
		expect(keyboardOcclusion()).toBe(300);

		// Keyboard grows taller: the visible band shrinks further.
		vv.height = 450;
		vv.fire("resize");

		expect(keyboardOcclusion()).toBe(350);
	});

	it("recomputes on a visualViewport scroll too, not just resize", async () => {
		const vv = makeFakeVisualViewport(0, 500);
		Object.defineProperty(window, "visualViewport", { value: vv, writable: true, configurable: true });
		const { keyboardOcclusion, ensureKeyboardViewportTracking } = await freshModule();
		ensureKeyboardViewportTracking();
		expect(keyboardOcclusion()).toBe(300);

		vv.offsetTop = 20;
		vv.fire("scroll");

		expect(keyboardOcclusion()).toBe(280);
	});

	it("is idempotent — a second call never re-registers the listeners", async () => {
		const vv = makeFakeVisualViewport(0, 500);
		Object.defineProperty(window, "visualViewport", { value: vv, writable: true, configurable: true });
		const { ensureKeyboardViewportTracking } = await freshModule();

		ensureKeyboardViewportTracking();
		ensureKeyboardViewportTracking();
		ensureKeyboardViewportTracking();

		expect(vv.listenerCounts.resize).toBe(1);
		expect(vv.listenerCounts.scroll).toBe(1);
	});
});
