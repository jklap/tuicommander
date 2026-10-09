import { afterEach, describe, expect, it } from "vitest";
import { settingsStore } from "../../stores/settings";
import { diffOptionsFromSettings } from "../../utils/diffOptionsFromSettings";

const SETTERS = {
	ignoreLeadingWs: (on: boolean) => settingsStore.setDiffIgnoreLeadingWhitespace(on),
	ignoreTrailingWs: (on: boolean) => settingsStore.setDiffIgnoreTrailingWhitespace(on),
	ignoreWsAmount: (on: boolean) => settingsStore.setDiffIgnoreWhitespaceAmount(on),
	ignoreCase: (on: boolean) => settingsStore.setDiffIgnoreCase(on),
} as const;

const ALL_OFF = { ignoreLeadingWs: false, ignoreTrailingWs: false, ignoreWsAmount: false, ignoreCase: false };

afterEach(() => {
	for (const set of Object.values(SETTERS)) set(false);
});

describe("diffOptionsFromSettings", () => {
	it("is all-off with the default settings", () => {
		expect(diffOptionsFromSettings()).toEqual(ALL_OFF);
	});

	// Catches: two fields read from the wrong (or the same) setting.
	it.each(Object.keys(SETTERS) as (keyof typeof SETTERS)[])("maps only its own setting onto %s", (field) => {
		SETTERS[field](true);
		expect(diffOptionsFromSettings()).toEqual({ ...ALL_OFF, [field]: true });
	});

	it("reads the live settings on every call rather than a snapshot", () => {
		expect(diffOptionsFromSettings().ignoreCase).toBe(false);
		settingsStore.setDiffIgnoreCase(true);
		expect(diffOptionsFromSettings().ignoreCase).toBe(true);
	});
});
