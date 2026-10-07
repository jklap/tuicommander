import { describe, expect, it } from "vitest";
import { tabletAppDestination } from "../utils/tabletRouting";

describe("tablet app routing", () => {
	// Catches: desktop-mode iPad Safari mounts the desktop UI instead of the touch UI.
	it("routes a touch-capable Mac UA and preserves navigation parameters", () => {
		expect(tabletAppDestination("Macintosh Safari", 5, "/", "?shared=k", "#sessions", false)).toBe(
			"/mobile?shared=k#sessions",
		);
		expect(tabletAppDestination("iPad Safari", 5, "/", "", "", false)).toBe("/mobile");
	});
	// Catches: ordinary Mac browsers or native WebViews get redirected away from desktop features.
	it("keeps desktop and native clients on their current shell", () => {
		expect(tabletAppDestination("Macintosh Safari", 0, "/", "", "", false)).toBeNull();
		expect(tabletAppDestination("Macintosh Safari", 5, "/", "", "", true)).toBeNull();
		expect(tabletAppDestination("Android", 5, "/", "", "", false)).toBeNull();
	});
	// Catches: routing loops or an iPad secret-form link loses its standalone UI.
	it("keeps mobile routes and secret forms intact", () => {
		expect(tabletAppDestination("Macintosh Safari", 5, "/mobile", "", "", false)).toBeNull();
		expect(tabletAppDestination("Macintosh Safari", 5, "/", "", "#/secret-form?request_id=a", false)).toBeNull();
	});
});
