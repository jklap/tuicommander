import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * The plugin fs-watch event name is built independently on both sides of the
 * IPC boundary: Rust formats it in `plugin_watch_path`, TypeScript rebuilds it
 * in `pluginRegistry`. Nothing links the two but the string itself, so a rename
 * on either side silently delivers a plugin's watch events to a listener that
 * does not exist — no error, no log, the plugin just stops seeing changes.
 *
 * That is exactly how it broke: the name was once keyed on the *plugin*, so a
 * plugin with K watches woke all K callbacks for every change (K-1 of them
 * wrong). Story 629-2277 re-keyed it on the watch id. The fan-out behaviour is
 * covered by `pluginRegistry.test.ts` ("delivers a watch's events to that
 * watch's callback only"), but that test mocks the emit and hardcodes the name,
 * so it cannot see the two sides drift apart.
 *
 * A source scan rather than a runtime test, because the real delivery needs an
 * `AppHandle`: `plugin_watch_path` is `#[cfg(feature = "desktop")]` and has no
 * HTTP equivalent by design, so no headless test can observe the round trip.
 */
describe("plugin fs-change event name", () => {
	const repoRoot = process.cwd();
	const rustSource = readFileSync(join(repoRoot, "src-tauri/src/plugin_fs.rs"), "utf8");
	const tsSource = readFileSync(join(repoRoot, "src/plugins/pluginRegistry.ts"), "utf8");

	it("is keyed on the watch id on the Rust side", () => {
		expect(rustSource).toContain('format!("plugin-fs-change-{watch_id}")');
	});

	it("is keyed on the watch id on the TypeScript side", () => {
		// biome-ignore lint/suspicious/noTemplateCurlyInString: the literal source text is the assertion
		expect(tsSource).toContain("`plugin-fs-change-${watchId}`");
	});

	it("agrees between the two sides", () => {
		// Reduce each side to the same shape — literal prefix plus the name of the
		// variable interpolated — so a rename of either the prefix or the key is a
		// failure here rather than a plugin that quietly stops receiving events.
		const rustName = rustSource.match(/format!\("(plugin-fs-change-)\{(\w+)\}"\)/);
		const tsName = tsSource.match(/`(plugin-fs-change-)\$\{(\w+)\}`/);

		expect(rustName, "no plugin-fs-change format! in plugin_fs.rs").not.toBeNull();
		expect(tsName, "no plugin-fs-change template in pluginRegistry.ts").not.toBeNull();

		// Same literal prefix.
		expect(tsName?.[1]).toBe(rustName?.[1]);
		// Same key, allowing for the snake_case/camelCase spelling of one id.
		expect(tsName?.[2]).toBe(rustName?.[2].replace(/_(\w)/g, (_, c) => c.toUpperCase()));
	});

	it("is not keyed on the plugin id on either side", () => {
		// The regression this story fixed. Either spelling reintroduces the K-watches
		// K-wakeups fan-out.
		expect(rustSource).not.toContain('format!("plugin-fs-change-{plugin_id}")');
		// biome-ignore lint/suspicious/noTemplateCurlyInString: the literal source text is the assertion
		expect(tsSource).not.toContain("`plugin-fs-change-${pluginId}`");
	});
});
