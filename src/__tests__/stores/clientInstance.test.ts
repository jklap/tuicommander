import { beforeEach, describe, expect, it, vi } from "vitest";

const STORAGE_KEY = "tuic.clientInstanceId";
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

describe("clientInstance", () => {
	beforeEach(() => {
		localStorage.clear();
		vi.resetModules();
	});

	it("mints a valid UUID on first load and persists it", async () => {
		const { CLIENT_INSTANCE_ID } = await import("../../stores/clientInstance");
		expect(CLIENT_INSTANCE_ID).toMatch(UUID_RE);
		expect(localStorage.getItem(STORAGE_KEY)).toBe(CLIENT_INSTANCE_ID);
	});

	it("is stable across reloads (a fresh module load reuses the persisted id)", async () => {
		const first = (await import("../../stores/clientInstance")).CLIENT_INSTANCE_ID;
		vi.resetModules();
		const second = (await import("../../stores/clientInstance")).CLIENT_INSTANCE_ID;
		expect(second).toBe(first);
	});

	// jsdom's `localStorage` instance owns `getItem`/`setItem` directly
	// (`localStorage.setItem !== Storage.prototype.setItem`), so the mock
	// must target the instance itself, not the prototype.

	// The mock must be set up BEFORE the dynamic `import()` in each of these,
	// not after: importing the module for the first time (post-`resetModules`)
	// itself evaluates the top-level `CLIENT_INSTANCE_ID = resolveClientInstanceId()`
	// side effect, which would otherwise run unmocked and persist a real id
	// before the test's own explicit call is ever made.

	it("still mints a usable id when localStorage.getItem throws", async () => {
		const spy = vi.spyOn(localStorage, "getItem").mockImplementation(() => {
			throw new Error("blocked");
		});
		const { resolveClientInstanceId } = await import("../../stores/clientInstance");
		expect(resolveClientInstanceId()).toMatch(UUID_RE);
		spy.mockRestore();
	});

	it("still mints a usable id when localStorage.setItem throws (private window / cleared site data)", async () => {
		const spy = vi.spyOn(localStorage, "setItem").mockImplementation(() => {
			throw new Error("blocked");
		});
		const { resolveClientInstanceId } = await import("../../stores/clientInstance");
		const id = resolveClientInstanceId();
		expect(id).toMatch(UUID_RE);
		// The write failed, so nothing was actually persisted — expected, not a bug.
		expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
		spy.mockRestore();
	});

	it("mints a fresh id on every call when localStorage.setItem always throws", async () => {
		const spy = vi.spyOn(localStorage, "setItem").mockImplementation(() => {
			throw new Error("blocked");
		});
		const { resolveClientInstanceId } = await import("../../stores/clientInstance");
		const first = resolveClientInstanceId();
		const second = resolveClientInstanceId();
		// Can't persist, so nothing carries the id from one resolution to the
		// next — this is the documented degrade-to-"mint a fresh id" behavior,
		// not a bug.
		expect(second).not.toBe(first);
		expect(second).toMatch(UUID_RE);
		spy.mockRestore();
	});
});
