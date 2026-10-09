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
		expect(eager).toContain("src/stores/settings.ts");
		expect(eager).toContain("src/i18n/locale.ts");
	});

	// en.json alone is ~20 KB gzip — a fifth of mobile.html's 100 KiB budget.
	// The mobile UI never calls t(); it only reaches the locale *setter* through
	// stores/settings.ts, which must import the catalog-free `i18n/locale`.
	it("does not load the i18n message catalog", () => {
		expect(eager.filter((f) => /^src\/i18n\/(t\.ts|index\.ts|.*\.json)$/.test(f))).toEqual([]);
	});
});
