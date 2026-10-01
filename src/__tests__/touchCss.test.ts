import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

// Computed-style checks of the touch stylesheets (#1329-a31a). happy-dom has no
// finger, so what a tablet scrolls is read from the cascade the browser would
// apply. Each case names the tablet bug it catches.

const css = (file: string) => readFileSync(resolve(__dirname, "..", file), "utf8");

afterEach(() => {
	document.head.innerHTML = "";
	document.body.innerHTML = "";
});

/** Index just past the `}` that closes the block opened at `open`. */
const blockEnd = (source: string, open: number) => {
	let depth = 0;
	for (let i = open; i < source.length; i++) {
		if (source[i] === "{") depth++;
		else if (source[i] === "}" && --depth === 0) return i + 1;
	}
	throw new Error("unbalanced css");
};

/**
 * happy-dom cannot evaluate `(hover: ...)`, so resolve the queries for a touch
 * device by hand: `(hover: none)` blocks apply (unwrapped), `(hover: hover)` blocks do not.
 */
const forTouchDevice = (source: string) => {
	let out = source;
	for (;;) {
		const m = /@media \(hover: (none|hover)\)\s*\{/.exec(out);
		if (!m) return out;
		const open = m.index + m[0].length - 1;
		const end = blockEnd(out, open);
		const body = m[1] === "none" ? out.slice(open + 1, end - 1) : "";
		out = out.slice(0, m.index) + body + out.slice(end);
	}
};

const addCss = (source: string) => {
	const style = document.createElement("style");
	style.textContent = forTouchDevice(source);
	document.head.appendChild(style);
};

describe("touch stylesheet contract (#1329-a31a)", () => {
	// Catches: the global touch-action blocking panning (iOS) or pinch-zoom on a
	// touch device, and any panel element inheriting a stricter value from it.
	it("an ordinary element on a touch device can pan and pinch-zoom", () => {
		addCss(css("global.css"));
		document.body.innerHTML = '<div id="panel"><div id="row"></div></div>';
		const value = getComputedStyle(document.getElementById("row") as HTMLElement).touchAction;
		expect(value).toBe("pan-x pan-y pinch-zoom");
	});

	// Catches: the branch + (and its long-press agent list) hidden behind :hover
	// and therefore unreachable on a tablet.
	it("sidebar branch actions are visible and hit-testable on a touch device", () => {
		addCss(css("components/Sidebar/Sidebar.module.css"));
		document.body.innerHTML = '<div class="branchActions"></div>';
		const style = getComputedStyle(document.querySelector(".branchActions") as HTMLElement);
		expect(style.opacity).toBe("1");
		expect(style.pointerEvents).toBe("auto");
	});
});
