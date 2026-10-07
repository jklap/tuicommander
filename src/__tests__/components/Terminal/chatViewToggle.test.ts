import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Chat mode must HIDE the grid and never unmount it: `CanvasTerminal` under a
 * disposed `<Show keyed>` with a frame event already queued froze the whole UI
 * (see the comment above that `<Show>` in Terminal.tsx). A source scan, like
 * canvasTerminalMountGuards.test.ts: mounting `Terminal` needs the canvas stack,
 * and the property is which wrapper the mode touches.
 */
describe("chat view toggle", () => {
	const source = readFileSync(join(process.cwd(), "src/components/Terminal/Terminal.tsx"), "utf8");
	const css = readFileSync(join(process.cwd(), "src/components/Terminal/Terminal.module.css"), "utf8");

	// Catches: the Show-keyed disposal freeze / lost scroll when switching to chat.
	it("toggle_hides_canvas_without_unmounting_it", () => {
		// The mode toggles a class on the container...
		expect(source).toMatch(
			/<div ref=\{containerRef\} class=\{s\.content\} classList=\{\{ \[s\.contentHidden\]: chatActive\(\) \}\}>/,
		);
		expect(css).toMatch(/\.contentHidden\s*\{\s*display:\s*none;\s*\}/);
		// ...and the Show that owns CanvasTerminal still depends on the session alone.
		const end = source.indexOf("<CanvasTerminal\n");
		const start = source.lastIndexOf("when={_currentSessionId()}", end);
		expect(start).toBeGreaterThan(-1);
		expect(end).toBeGreaterThan(start);
		expect(source.slice(start, end)).not.toMatch(/viewMode|chatActive/);
	});
});
