import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import commander from "../../src-tauri/src/themes/commander.json";
import paper from "../../src-tauri/src/themes/vscode-light.json";
import { contrastRatio } from "../themes";

const sidebarCss = readFileSync("src/components/Sidebar/Sidebar.module.css", "utf8");
const globalCss = readFileSync("src/global.css", "utf8");

// Resolve the shipped badge declarations and their flat CSS token scopes without
// claiming browser rendering. Theme application overrides --text-on-accent.
function declarations(css: string, selector: string): Record<string, string> {
	const block = [...css.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/([^{}]+)\{([^{}]*)\}/g)].find(
		(match) => match[1].trim() === selector,
	)?.[2];
	if (!block) throw new Error(`Missing CSS scope: ${selector}`);
	return Object.fromEntries(
		[...block.matchAll(/([\w-]+)\s*:\s*([^;]+);/g)].map((match) => [match[1], match[2].trim()]),
	);
}

function resolve(value: string, tokens: Record<string, string>): string {
	const token = /^var\((--[\w-]+)\)$/.exec(value)?.[1];
	if (!token) return value;
	if (!tokens[token]) throw new Error(`Missing CSS token: ${token}`);
	return resolve(tokens[token], tokens);
}

describe("filled merged PR badge contrast", () => {
	// Catches: a marker palette change leaving small filled labels on the
	// theme's unrelated accent foreground (white on bright purple in Commander).
	it.each([
		{ name: "Commander", theme: commander, selector: undefined },
		{ name: "Paper", theme: paper, selector: '[data-theme="vscode-light"]' },
	])("keeps $name labels at WCAG AA instead of inheriting an unreadable accent foreground", ({ theme, selector }) => {
		const tokens = {
			...declarations(globalCss, ":root"),
			...(selector ? declarations(globalCss, selector) : {}),
			"--text-on-accent": theme.appChrome.accentForeground,
		};
		const badge = declarations(sidebarCss, ".prMerged");
		const foreground = resolve(badge.color, tokens);
		const background = resolve(badge.background, tokens);
		expect(contrastRatio(foreground, background)).toBeGreaterThanOrEqual(4.5);
	});
});
