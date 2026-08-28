import { createEffect } from "solid-js";
import { applyIndicatorOverrides } from "../indicators/apply";
import { settingsStore } from "../stores/settings";
import { applyAppTheme, applyFontFamily, themesLoaded } from "../themes";

/** Synchronizes reactive appearance settings with document-level CSS and theme state. */
export function useAppearanceSync(): void {
	createEffect(() => {
		const theme = settingsStore.state.theme;
		if (themesLoaded()) applyAppTheme(theme);
	});
	createEffect(() => applyFontFamily(settingsStore.state.font));
	// applyAppTheme (above) re-applies overrides too, but only runs on a
	// theme change — this covers editing an override without touching the
	// theme (the common case once the legend editor ships).
	//
	// Reading just `settingsStore.state.indicatorOverrides` (the array
	// reference) does NOT track an append or an in-place field mutation —
	// Solid's store proxy only notifies a plain property read when the key
	// itself is reassigned wholesale (as clearIndicatorOverride/
	// resetAllIndicators do), not when setIndicatorColor mutates an index or
	// adds one. `.map()` here iterates every index and spreads every item,
	// which reads `.length` and each field — establishing a real dependency
	// on the array's shape AND contents, not just its identity.
	createEffect(() => {
		const overrides = settingsStore.state.indicatorOverrides.map((o) => ({ ...o }));
		applyIndicatorOverrides(overrides);
	});
	// Tab-type tint toggle — CSS reads this attribute (TabBar.module.css,
	// PaneTree.css) to neutralize the per-type background/border while
	// keeping each type's icon color (which the same type class also
	// carries, so it can't just be dropped in TSX).
	createEffect(() => {
		document.documentElement.dataset.tabTypeTint = settingsStore.state.tabTypeHighlighting ? "on" : "off";
	});
}
