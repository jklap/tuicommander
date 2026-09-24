import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";

vi.mock("../../stores/ui", async () => {
	const { createStore } = await import("solid-js/store");
	const [state, setState] = createStore({ settingsExpertMode: false });
	return {
		uiStore: {
			state,
			setSettingsExpertMode: vi.fn((enabled: boolean) => setState("settingsExpertMode", enabled)),
		},
	};
});

import { settingsExpertStore } from "../../stores/settingsExpert";
import { uiStore } from "../../stores/ui";

const KEY = "app.osc52_clipboard";
const DEFAULTS = { app: { osc52_clipboard: true }, notifications: {}, agent_settings: {} };

function serveDefaults(): void {
	mockInvoke.mockImplementation((cmd: string) =>
		cmd === "get_config_defaults" ? Promise.resolve(DEFAULTS) : Promise.resolve(undefined),
	);
}

describe("settingsExpertStore visibility within one Settings open", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		settingsExpertStore._resetForTests();
		uiStore.setSettingsExpertMode(false);
		serveDefaults();
	});

	it("keeps a control visible after its value returns to the default, until the next open()", async () => {
		// Resetting an override must not make the control vanish under the user's cursor.
		await settingsExpertStore.open();
		expect(settingsExpertStore.isVisible(KEY, false)).toBe(true);

		expect(settingsExpertStore.isVisible(KEY, true)).toBe(true);

		await settingsExpertStore.open();
		expect(settingsExpertStore.isVisible(KEY, true)).toBe(false);
	});

	it("does not pin a control that was visible only because defaults were still loading", async () => {
		// Pinning here would show every expert control on the first open.
		let resolveDefaults: (value: unknown) => void = () => {};
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "get_config_defaults"
				? new Promise((resolve) => {
						resolveDefaults = resolve;
					})
				: Promise.resolve(undefined),
		);
		const opening = settingsExpertStore.open();
		expect(settingsExpertStore.isVisible(KEY, true)).toBe(true);

		resolveDefaults(DEFAULTS);
		await opening;
		expect(settingsExpertStore.isVisible(KEY, true)).toBe(false);
	});

	it("hides an at-default control when expert mode is switched off in the same open", async () => {
		// The Expert switch is an explicit request to hide; it must act immediately.
		await settingsExpertStore.open();
		uiStore.setSettingsExpertMode(true);
		expect(settingsExpertStore.isVisible(KEY, true)).toBe(true);

		uiStore.setSettingsExpertMode(false);
		expect(settingsExpertStore.isVisible(KEY, true)).toBe(false);
	});

	it("does not keep a control visible that was never visible this open", async () => {
		await settingsExpertStore.open();
		expect(settingsExpertStore.isVisible(KEY, true)).toBe(false);
		expect(settingsExpertStore.isVisible(KEY, true)).toBe(false);
	});
});
