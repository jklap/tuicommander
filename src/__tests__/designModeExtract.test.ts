import { readFileSync } from "node:fs";
import { join } from "node:path";
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";

type Extracted = {
	url: string;
	selector: string;
	elementPath: string;
	fullPath: string;
	nearbyText: string;
	attributes: Record<string, string>;
	htmlSnippet: string;
	styles: Record<string, string>;
	rect: Record<string, number>;
	source: {
		reactStack?: string;
		reactSource?: { fileName: string; lineNumber: number };
		vueFile?: string;
		svelteLoc?: { file: string; line: number; column: number };
	};
};

const source = readFileSync(join(process.cwd(), "src-tauri/src/design_mode/extract.js"), "utf8");

function extract(html: string, target: string, setup?: (node: Element) => void): Extracted & { pageMutated: boolean } {
	const dom = new JSDOM(html);
	const node = dom.window.document.querySelector(target);
	if (!node) throw new Error(`Missing test node ${target}`);
	setup?.(node);
	const fn = dom.window.eval(`(${source.trim()})`) as (this: Element) => Extracted;
	const before = dom.window.document.documentElement.outerHTML;
	const result = fn.call(node);
	return { ...result, pageMutated: dom.window.document.documentElement.outerHTML !== before };
}

describe("design mode extraction", () => {
	it("chooses a unique ID over classes and does not mutate the page", () => {
		const html = `<div><button id="save" class="stable">Save</button><button class="stable">Other</button></div>`;
		const result = extract(html, "#save");
		expect(result.selector).toBe("#save");
		expect(result.attributes).toEqual({ id: "save", class: "stable" });
		expect(result.htmlSnippet).toContain("Save");
		// The pick runs inside the user's live page: extraction must leave no trace.
		expect(result.pageMutated).toBe(false);
	});

	it("uses stable classes, then nth-of-type when hashes collide", () => {
		const html = `<main><button class="css-a1B2c3 stable">A</button><button class="css-a1B2c3">B</button><button class="css-a1B2c3">C</button></main>`;
		expect(extract(html, "button:first-child").selector).toContain(".stable");
		const last = extract(html, "button:last-child");
		expect(last.selector).toContain(":nth-of-type(3)");
		expect(last.selector).not.toContain("css-a1B2c3");
	});

	it("bounds readable paths and nearby text on a deep large page", () => {
		const nested = `${"<section>".repeat(23)}<button>${"x".repeat(5000)}</button>${"</section>".repeat(23)}`;
		const result = extract(nested, "button");
		expect(result.elementPath.split(" > ")).toHaveLength(6);
		expect(result.fullPath.split(" > ").length).toBeLessThanOrEqual(20);
		expect(result.nearbyText.length).toBeLessThanOrEqual(500);
	});

	it("returns framework hints without resolving them in the page", () => {
		const result = extract("<div id='node'></div>", "#node", (node) => {
			Object.assign(node, {
				__reactFiber$test: {
					_debugStack: { stack: "Error\n at jsxDEV\n at App" },
					_debugSource: { fileName: "App.tsx", lineNumber: 12 },
				},
				__vueParentComponent: { type: { __file: "Vue.vue" } },
				__svelte_meta: { loc: { file: "App.svelte", line: 4, column: 2 } },
			});
		});
		expect(result.source).toEqual({
			reactStack: "Error\n at jsxDEV\n at App",
			reactSource: { fileName: "App.tsx", lineNumber: 12 },
			vueFile: "Vue.vue",
			svelteLoc: { file: "App.svelte", line: 4, column: 2 },
		});
	});

	it("collects a bounded style subset and the clicked element rectangle", () => {
		const result = extract("<button style='color: red; font-size: 16px'>Pick</button>", "button");
		expect(result.url).toBe("about:blank");
		expect(result.styles.color).toBe("rgb(255, 0, 0)");
		expect(result.styles.fontSize).toBe("16px");
		expect(Object.keys(result.styles).length).toBeLessThanOrEqual(13);
		// DEFERRED (2026-09-23) — review TEST-1(c): Rust discards this `rect` and uses
		// the CDP box model (manager.rs). Drop the field from extract.js (a backend asset)
		// together with this assertion; it only proves jsdom returns zeros.
		expect(result.rect).toEqual({ x: 0, y: 0, width: 0, height: 0 });
	});

	it("limits attributes before sending the node over CDP", () => {
		const attrs = Array.from({ length: 60 }, (_, index) => `data-field-${index}='x'`).join(" ");
		const result = extract(`<button ${attrs}>Pick</button>`, "button");
		expect(Object.keys(result.attributes)).toHaveLength(32);
	});
});
