import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({
	invoke: vi.fn(() => Promise.resolve(null)),
}));

import { settingsRefresh } from "../../services/settingsRefresh";
import { paneLayoutStore, resetGroupCounter } from "../../stores/paneLayout";
import { testInScope } from "../helpers/store";

// The backend stores pane-layout.json as untyped JSON with merge-patch semantics: a `null`
// key is a delete (`apply_json_merge_delta`), so a reset layout (`root: null`) is written to
// disk WITHOUT a `root` key, and `load_pane_layout` hands that back verbatim. Crash seen in
// the field: "undefined is not an object (evaluating 'u.type')" in `isSplit()`.
describe("paneLayoutStore.startRemoteSync", () => {
	let apply: ((value: unknown) => void) | undefined;
	let stop: (() => void) | undefined;

	beforeEach(() => {
		paneLayoutStore.reset();
		resetGroupCounter();
		apply = undefined;
		vi.spyOn(settingsRefresh, "register").mockImplementation((reader) => {
			if (reader.section === "pane_layout") apply = reader.apply as (value: unknown) => void;
			return () => {};
		});
		stop = paneLayoutStore.startRemoteSync();
	});

	afterEach(() => {
		stop?.();
		paneLayoutStore._testCancelPendingSave();
		vi.restoreAllMocks();
	});

	it("adopts an on-disk layout with no root key without throwing from isSplit()", () => {
		testInScope(() => {
			expect(apply).toBeDefined();
			// What the backend actually holds after a reset: root and activeGroupId deleted.
			expect(() => apply?.({ groups: {} })).not.toThrow();
			expect(paneLayoutStore.isSplit()).toBe(false);
			expect(paneLayoutStore.getRoot()).toBeNull();
		});
	});

	it("adopts a completely empty on-disk layout ({})", () => {
		testInScope(() => {
			expect(() => apply?.({})).not.toThrow();
			expect(paneLayoutStore.isSplit()).toBe(false);
			expect(paneLayoutStore.getRoot()).toBeNull();
			expect(paneLayoutStore.state.groups).toEqual({});
			expect(paneLayoutStore.state.activeGroupId).toBeNull();
		});
	});

	it("treats a null remote layout like an empty one", () => {
		testInScope(() => {
			expect(() => apply?.(null)).not.toThrow();
			expect(paneLayoutStore.isSplit()).toBe(false);
		});
	});

	it("keeps an unsaved local split when the remote layout has no root", () => {
		testInScope(() => {
			const g1 = paneLayoutStore.createGroup();
			paneLayoutStore.split(g1, "vertical");
			expect(paneLayoutStore.isSplit()).toBe(true);

			expect(() => apply?.({ groups: {} })).not.toThrow();
			// Remote has nothing to say about the tree, so local intent wins — and the
			// store must stay in a state isSplit() can read.
			expect(paneLayoutStore.isSplit()).toBe(true);
		});
	});

	it("adopts a remote split over an untouched local layout", () => {
		testInScope(() => {
			apply?.({
				root: {
					type: "branch",
					direction: "vertical",
					ratios: [0.5, 0.5],
					children: [
						{ type: "leaf", id: "g1" },
						{ type: "leaf", id: "g2" },
					],
				},
				groups: {
					g1: { id: "g1", tabs: [], activeTabId: null },
					g2: { id: "g2", tabs: [], activeTabId: null },
				},
				activeGroupId: "g1",
			});
			expect(paneLayoutStore.isSplit()).toBe(true);
			expect(paneLayoutStore.state.activeGroupId).toBe("g1");
		});
	});
});
