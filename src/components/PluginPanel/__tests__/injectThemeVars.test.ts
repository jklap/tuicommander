import { describe, expect, it } from "vitest";
import { hasOwnStyling, injectThemeVars } from "../PluginPanel";

describe("hasOwnStyling", () => {
	it("detects an inline <style> block", () => {
		expect(hasOwnStyling("<html><head><style>body{color:red}</style></head></html>")).toBe(true);
		expect(hasOwnStyling('<style type="text/css">a{}</style>')).toBe(true);
	});

	it("detects a linked stylesheet regardless of attribute order/quoting", () => {
		expect(hasOwnStyling('<link rel="stylesheet" href="x.css">')).toBe(true);
		expect(hasOwnStyling("<link href='x.css' rel=stylesheet>")).toBe(true);
	});

	it("returns false for plain unstyled HTML (incl. dashboard markup)", () => {
		expect(hasOwnStyling("<div>hello</div>")).toBe(false);
		expect(hasOwnStyling('<div class="dashboard"><h1 class="dash-title">x</h1></div>')).toBe(false);
		// A stray word "style" in text must not trip detection.
		expect(hasOwnStyling("<p>the style of this page</p>")).toBe(false);
	});
});

describe("injectThemeVars base-sheet scoping (#080)", () => {
	it("does NOT inject the tuic-base sheet when the document is self-styled", () => {
		const out = injectThemeVars(
			"<html><head><style>body{background:#fff;color:#000;font-size:20px}</style></head><body>hi</body></html>",
		);
		expect(out).not.toContain('id="tuic-base"');
		// The document's own style is preserved untouched.
		expect(out).toContain("background:#fff");
	});

	it("does NOT inject the tuic-base sheet when the document links a stylesheet", () => {
		const out = injectThemeVars('<html><head><link rel="stylesheet" href="a.css"></head><body>hi</body></html>');
		expect(out).not.toContain('id="tuic-base"');
	});

	it("injects the tuic-base sheet for plain unstyled HTML", () => {
		const out = injectThemeVars('<html><head></head><body><div class="dashboard">x</div></body></html>');
		expect(out).toContain('id="tuic-base"');
	});

	it("still injects the SDK script in both branches", () => {
		const styled = injectThemeVars("<style>a{}</style><body>x</body>");
		const plain = injectThemeVars("<body>x</body>");
		expect(styled).toContain('id="tuic-sdk"');
		expect(plain).toContain('id="tuic-sdk"');
	});
});
