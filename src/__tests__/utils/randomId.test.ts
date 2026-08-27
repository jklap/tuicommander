import { afterEach, describe, expect, it, vi } from "vitest";
import { randomId, randomUuid } from "../../utils/randomId";

describe("randomId", () => {
	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it("prefixes the crypto UUID when one is available", () => {
		vi.stubGlobal("crypto", { randomUUID: () => "1111-2222" });
		expect(randomId("cs-")).toBe("cs-1111-2222");
	});

	/// The reason this helper exists: `crypto.randomUUID` is only defined in a
	/// secure context, and TUIC is reached over plain http on a LAN address. A
	/// bare call would throw for exactly the remote clients that need the id.
	it("still produces an id when crypto.randomUUID is missing", () => {
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
	it("never emits a slash, in either branch", () => {
		vi.stubGlobal("crypto", {});
		const ids = Array.from({ length: 50 }, () => randomId("c"));
		expect(ids.some((id) => id.includes("/"))).toBe(false);
		// Distinct enough that two clients of one backend do not collide.
		expect(new Set(ids).size).toBeGreaterThan(1);
	});
});

// Backend fields typed as `uuid::Uuid` (tuicSession / $TUIC_SESSION's
// `is_valid_uuid` guard, RemoteConnection and upstream MCP ids) need canonical
// shape in every branch, including plain-http LAN clients without randomUUID.
describe("randomUuid", () => {
	const UUID_V4_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it("returns crypto.randomUUID verbatim when available", () => {
		vi.stubGlobal("crypto", { randomUUID: () => "01a13487-f2a9-4480-90b8-d21e87833dc2" });
		expect(randomUuid()).toBe("01a13487-f2a9-4480-90b8-d21e87833dc2");
	});

	// `getRandomValues` has no secure-context restriction, so this is the
	// realistic non-secure-context path.
	it("builds a canonical v4 UUID from getRandomValues when randomUUID is missing", () => {
		vi.stubGlobal("crypto", {
			getRandomValues: (arr: Uint8Array) => {
				arr.fill(0xab);
				return arr;
			},
		});
		expect(randomUuid()).toMatch(UUID_V4_RE);
	});

	it("getRandomValues ids are distinct across calls", () => {
		const realCrypto = globalThis.crypto;
		vi.stubGlobal("crypto", { getRandomValues: realCrypto.getRandomValues.bind(realCrypto) });
		const ids = Array.from({ length: 50 }, () => randomUuid());
		expect(new Set(ids).size).toBe(50);
		for (const id of ids) expect(id).toMatch(UUID_V4_RE);
	});

	it("keeps UUID shape even without any crypto", () => {
		vi.stubGlobal("crypto", undefined);
		expect(randomUuid()).toMatch(UUID_V4_RE);
	});
});
