// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";

const inputCss = readFileSync("src/mobile/components/CommandInput.module.css", "utf8");

vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	rpc: vi.fn(async () => ({})),
}));
vi.mock("../useMobileVoice", () => ({ useMobileVoice: () => ({ available: () => false }) }));

afterEach(cleanup);

// Conditional CSS inputs, not a claim that jsdom reproduces an iOS viewport.
// Catches: the composer omits the Home Indicator inset or replaces its base padding.
it.each([
	[0, ["4px", "calc(4px)", "calc(4px + 0px)"]],
	[34, ["38px", "calc(38px)", "calc(4px + 34px)"]],
] as const)("keeps the %ipx bottom inset inside the painted composer", (inset, expectedPadding) => {
	const view = render(() => (
		<>
			<style>{inputCss.replaceAll(/env\(safe-area-inset-bottom(?:,\s*0px)?\)/g, `${inset}px`)}</style>
			<CommandInput sessionId="safe-area-test" />
		</>
	));
	const composer = view.getByRole("textbox").closest(".form")!;
	const style = getComputedStyle(composer);
	// CSSOM may serialize the sum or simplify it; both express the same fixed budget.
	expect(expectedPadding).toContain(style.paddingBottom);
	expect(Number.parseFloat(style.marginBottom || "0")).toBe(0);
	expect(style.flexShrink).toBe("0");
});
