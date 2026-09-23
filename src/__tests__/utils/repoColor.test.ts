import { describe, expect, it, vi } from "vitest";

const colors: Record<string, string | undefined> = { "/r/red": "#ff6b6b" };
vi.mock("../../stores/repoSettings", () => ({
	repoSettingsStore: { get: (path: string) => (colors[path] ? { color: colors[path] } : undefined) },
}));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: { getGroupForRepo: () => undefined },
}));

const { getRepoColor, getRepoTextColor } = await import("../../utils/repoColor");

describe("getRepoTextColor", () => {
	it("shades the picked color by the theme's --repo-text-shade, defaulting to no shade", () => {
		// The default keeps dark themes on the exact picked color; only a theme
		// that sets the variable (a light one) darkens repo names.
		expect(getRepoTextColor("/r/red")).toBe("color-mix(in oklab, #ff6b6b, #000 var(--repo-text-shade, 0%))");
	});

	it("leaves the swatch color untouched", () => {
		expect(getRepoColor("/r/red")).toBe("#ff6b6b");
	});

	it("returns undefined for a repo without a color, so the name keeps the theme foreground", () => {
		expect(getRepoTextColor("/r/none")).toBeUndefined();
	});
});
