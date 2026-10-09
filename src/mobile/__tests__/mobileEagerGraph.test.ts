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
		expect(eager).toContain("src/mobile/screens/SessionsScreen.tsx");
		expect(eager).toContain("src/stores/appLogger.ts");
	});

	// en.json alone is ~20 KB gzip — a fifth of mobile.html's 100 KiB budget.
	// The mobile UI never calls t(). MobileApp keeps the screens that reach the
	// settings store lazy, and stores/settings.ts itself only imports the
	// catalog-free `i18n/locale`, so neither path may pull the catalog in.
	it("does not load the i18n message catalog", () => {
		expect(eager.filter((f) => /^src\/i18n\/(t\.ts|index\.ts|.*\.json)$/.test(f))).toEqual([]);
	});
});
