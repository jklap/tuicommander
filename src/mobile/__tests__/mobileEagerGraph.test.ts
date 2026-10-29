import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";
import { reachableModules } from "../../__tests__/helpers/importGraph";

const ROOT = process.cwd();

describe("mobile.html eager import graph", () => {
	const eager = reachableModules(join(ROOT, "src/mobile/index.tsx"), { includeDynamic: false }).map((f) =>
		relative(ROOT, f),
	);

	it("actually walks the mobile app (sanity)", () => {
		expect(eager).toContain("src/mobile/MobileApp.tsx");
		expect(eager).toContain("src/stores/appLogger.ts");
	});

	// en.json alone is ~20 KB gzip. The mobile UI never calls t(); code it reaches
	// eagerly that only sets the locale must import the catalog-free `i18n/locale`.
	const catalog = (files: string[]) => files.filter((f) => /^src\/i18n\/(t\.ts|index\.ts|.*\.json)$/.test(f));

	it("does not load the i18n message catalog", () => {
		expect(catalog(eager)).toEqual([]);
	});

	// Today no eager mobile module imports i18n at all, so the guard above would
	// pass vacuously if its matcher drifted from the catalog's real paths. Walking
	// from the desktop i18n entry must trip it.
	it("recognizes the catalog modules a walk that reaches them sees (sanity)", () => {
		const desktop = reachableModules(join(ROOT, "src/i18n/index.ts"), { includeDynamic: false }).map((f) =>
			relative(ROOT, f),
		);
		expect(catalog(desktop)).toEqual(expect.arrayContaining(["src/i18n/index.ts", "src/i18n/t.ts"]));
	});
});
