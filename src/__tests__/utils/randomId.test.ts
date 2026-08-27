import { afterEach, describe, expect, it, vi } from "vitest";
import { randomId } from "../../utils/randomId";

describe("randomId", () => {
	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it("prefixes the crypto UUID when one is available", () => {
		vi.stubGlobal("crypto", { randomUUID: () => "1111-2222" });
		expect(randomId("cs-")).toBe("cs-1111-2222");
	});

	// `crypto.getRandomValues` has no secure-context restriction — unlike
	// `randomUUID`, real non-secure-context browsers (the plain-http LAN case
	// this helper exists for) still have it. This is the realistic fallback
	// path, and some callers (e.g. a terminal's `tuicSession`) hand the result
	// straight to backend validators that require canonical UUID shape.
	const UUID_V4_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

	it("builds a canonical v4 UUID from getRandomValues when randomUUID is missing", () => {
		vi.stubGlobal("crypto", {
			getRandomValues: (arr: Uint8Array) => {
				arr.fill(0xab);
				return arr;
			},
		});
		const id = randomId("");
		expect(id).toMatch(UUID_V4_RE);
	});

	it("getRandomValues ids are distinct across calls", () => {
		const realCrypto = globalThis.crypto;
		vi.stubGlobal("crypto", { getRandomValues: realCrypto.getRandomValues.bind(realCrypto) });
		const ids = Array.from({ length: 50 }, () => randomId(""));
		expect(new Set(ids).size).toBe(50);
		for (const id of ids) expect(id).toMatch(UUID_V4_RE);
	});

	/// Only reached when even `getRandomValues` is absent — an exotic case with
	/// no real non-secure-context browser today, but kept as a last resort.
	it("still produces an id when both randomUUID and getRandomValues are missing", () => {
		vi.stubGlobal("crypto", {});
		const id = randomId("c");
		expect(id.startsWith("c")).toBe(true);
		expect(id.length).toBeGreaterThan(5);
	});

	it("survives a missing crypto object entirely", () => {
		vi.stubGlobal("crypto", undefined);
		expect(() => randomId("s")).not.toThrow();
	});

	/// The backend qualifies watcher ids with "/", so an id containing one is
	/// parsed as two.
	it("never emits a slash, in any branch", () => {
		vi.stubGlobal("crypto", {});
		const ids = Array.from({ length: 50 }, () => randomId("c"));
		expect(ids.some((id) => id.includes("/"))).toBe(false);
		// Distinct enough that two clients of one backend do not collide.
		expect(new Set(ids).size).toBeGreaterThan(1);
	});
});
