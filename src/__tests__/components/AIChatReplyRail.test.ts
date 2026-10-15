import { readFileSync } from "node:fs";
import { expect, it } from "vitest";

// CSS is the behavior boundary: jsdom does not compute pseudo-element styles.
it("does not paint an AI Chat answer rail — catches terminal decoration leaking into chat", () => {
	const css = readFileSync("src/components/AIChatPanel/AIChatPanel.module.css", "utf8");
	const answerRailRules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter(
		([, selector, declarations]) =>
			selector.includes("[data-tuic-answer]::after") && /background:\s*var\(--answer-rule\)/.test(declarations),
	);
	expect(answerRailRules).toHaveLength(0);
	expect(css).toMatch(/\[data-terminal-chat\] \[data-answer-card\]\s*\{[^}]*border-left: 3px solid var\(--success\)/);
});

// Catches: the 32px Stop sharing an 8px offset intended for 20px controls.
it("aligns desktop composer controls to the input height — catches Stop offset mismatch", () => {
	const css = readFileSync("src/components/AIChatPanel/AIChatPanel.module.css", "utf8");
	expect(css).toMatch(
		/\.inputArea:not\(\.mobileComposer\) > \.composerActions > button\s*\{[^}]*height: 36px;[^}]*margin-bottom: 0;/,
	);
});
