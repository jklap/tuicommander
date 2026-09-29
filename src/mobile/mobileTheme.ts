import { applyAppTheme, loadThemes, themesLoaded } from "../themes";

const MOBILE_THEME_KEY = "tuic-mobile-theme";

export function mobileTheme(): "commander" | "vscode-light" {
	return localStorage.getItem(MOBILE_THEME_KEY) === "vscode-light" ? "vscode-light" : "commander";
}

export async function loadMobileTheme(): Promise<void> {
	if (!themesLoaded()) await loadThemes();
	applyAppTheme(mobileTheme(), false);
}

export function setMobileTheme(theme: "commander" | "vscode-light"): void {
	localStorage.setItem(MOBILE_THEME_KEY, theme);
	applyAppTheme(theme, false);
}
