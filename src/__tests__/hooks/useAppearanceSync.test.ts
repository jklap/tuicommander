import { createRoot } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockThemes, mockApply } = vi.hoisted(() => ({
	mockThemes: {
		applyAppTheme: vi.fn(),
		applyFontFamily: vi.fn(),
		themesLoaded: vi.fn(() => true),
	},
	mockApply: {
		applyIndicatorOverrides: vi.fn(),
	},
}));

vi.mock("../../themes", () => ({
	applyAppTheme: mockThemes.applyAppTheme,
	applyFontFamily: mockThemes.applyFontFamily,
	themesLoaded: mockThemes.themesLoaded,
}));

vi.mock("../../indicators/apply", () => ({
	applyIndicatorOverrides: mockApply.applyIndicatorOverrides,
}));

import { useAppearanceSync } from "../../hooks/useAppearanceSync";
import { settingsStore } from "../../stores/settings";

/**
 * useCompositionLifecycles.test.ts covers this hook only incidentally (one
 * assertion, with settingsStore fully mocked as a static object — no
 * reactivity exercised). This file drives the real settingsStore so theme/font
 * CHANGES, not just the initial read, are verified to re-run the effects.
 */
describe("useAppearanceSync", () => {
	let dispose: (() => void) | undefined;
	const originalTheme = settingsStore.state.theme;
	const originalFont = settingsStore.state.font;

	beforeEach(() => {
		mockThemes.applyAppTheme.mockClear();
		mockThemes.applyFontFamily.mockClear();
		mockThemes.themesLoaded.mockReturnValue(true);
		mockApply.applyIndicatorOverrides.mockClear();
	});

	afterEach(() => {
		dispose?.();
		dispose = undefined;
		settingsStore.setTheme(originalTheme);
		settingsStore.setFont(originalFont);
		settingsStore.resetAllIndicators();
		settingsStore.setTabTypeHighlighting(true);
		delete document.documentElement.dataset.tabTypeTint;
	});

	it("applies the current theme and font on mount when themes are loaded", () => {
		settingsStore.setTheme("dracula");
		settingsStore.setFont("Fira Code");
		createRoot((rootDispose) => {
			dispose = rootDispose;
			useAppearanceSync();
		});

		expect(mockThemes.applyAppTheme).toHaveBeenCalledWith("dracula");
		expect(mockThemes.applyFontFamily).toHaveBeenCalledWith("Fira Code");
	});

	it("does not apply the theme when themes have not finished loading", () => {
		mockThemes.themesLoaded.mockReturnValue(false);
		createRoot((rootDispose) => {
			dispose = rootDispose;
			useAppearanceSync();
		});

		expect(mockThemes.applyAppTheme).not.toHaveBeenCalled();
	});

	it("re-applies the theme when settingsStore.state.theme changes", () => {
		createRoot((rootDispose) => {
			dispose = rootDispose;
			useAppearanceSync();
		});
		mockThemes.applyAppTheme.mockClear();

		settingsStore.setTheme("nord");

		expect(mockThemes.applyAppTheme).toHaveBeenCalledWith("nord");
	});

	it("re-applies the font when settingsStore.state.font changes", () => {
		createRoot((rootDispose) => {
			dispose = rootDispose;
			useAppearanceSync();
		});
		mockThemes.applyFontFamily.mockClear();

		settingsStore.setFont("Hack");

		expect(mockThemes.applyFontFamily).toHaveBeenCalledWith("Hack");
	});

	it("applies the current indicator overrides on mount", () => {
		settingsStore.setIndicatorColor("terminal.busy", "#ff00ff");
		createRoot((rootDispose) => {
			dispose = rootDispose;
			useAppearanceSync();
		});

		expect(mockApply.applyIndicatorOverrides).toHaveBeenCalledWith([{ id: "terminal.busy", color: "#ff00ff" }]);
	});

	it("re-applies overrides when they change without a theme change", () => {
		createRoot((rootDispose) => {
			dispose = rootDispose;
			useAppearanceSync();
		});
		mockApply.applyIndicatorOverrides.mockClear();

		settingsStore.setIndicatorColor("pr.conflict", "#00ff00");

		expect(mockApply.applyIndicatorOverrides).toHaveBeenCalledWith([{ id: "pr.conflict", color: "#00ff00" }]);
	});

	it("sets data-tab-type-tint=on when tabTypeHighlighting is on (the default) at mount", () => {
		createRoot((rootDispose) => {
			dispose = rootDispose;
			useAppearanceSync();
		});

		expect(document.documentElement.dataset.tabTypeTint).toBe("on");
	});

	it("sets data-tab-type-tint=off when tabTypeHighlighting is off at mount", () => {
		settingsStore.setTabTypeHighlighting(false);
		createRoot((rootDispose) => {
			dispose = rootDispose;
			useAppearanceSync();
		});

		expect(document.documentElement.dataset.tabTypeTint).toBe("off");
	});

	it("re-sets the attribute when tabTypeHighlighting changes after mount", () => {
		createRoot((rootDispose) => {
			dispose = rootDispose;
			useAppearanceSync();
		});
		expect(document.documentElement.dataset.tabTypeTint).toBe("on");

		settingsStore.setTabTypeHighlighting(false);

		expect(document.documentElement.dataset.tabTypeTint).toBe("off");
	});
});
