/** Real recorded Claude marker through the production mobile log renderer. */
import { render } from "solid-js/web";
import "../global.css";
import darkTheme from "../../src-tauri/src/themes/vscode-dark.json";
import lightTheme from "../../src-tauri/src/themes/vscode-light.json";
import { paintOverlayBlocks, planSuggestOverlay } from "../components/Terminal/suggestOverlay";
import { OutputView } from "../mobile/components/OutputView";
import claudeFixture from "./fixtures/claude-intent-2019.json";
import egoFixture from "./fixtures/ego-intent-2060.json";

const fixture = new URLSearchParams(location.search).get("fixture") === "2060" ? egoFixture : claudeFixture;

const theme = new URLSearchParams(location.search).get("theme") === "light" ? lightTheme : darkTheme;
for (const [key, value] of Object.entries({
	"--bg-primary": theme.appChrome.background,
	"--fg-primary": theme.appChrome.foreground,
	"--warning": theme.appChrome.warning,
}))
	document.documentElement.style.setProperty(key, value);
const chars = [...fixture.text];
const originalFetch = window.fetch.bind(window);
window.fetch = async (input, init) => {
	if (String(input).startsWith("/sessions/fixture-2019/output")) {
		return Response.json({
			lines: [],
			total_lines: 0,
			screen: [
				{ spans: [{ text: chars.slice(0, 40).join("") }], cols: 40, wrapped: true },
				{ spans: [{ text: chars.slice(40).join("") }], cols: 40, wrapped: false },
			],
		});
	}
	return originalFetch(input, init);
};
const root = document.getElementById("mobile-intent-harness");
if (!root) throw new Error("Harness mount missing");
root.style.cssText = "display:flex;width:100%;height:100dvh;background:var(--bg-primary)";
document.body.style.margin = "0";
if (new URLSearchParams(location.search).has("desktop")) {
	// Recorded model text, laid out into physical rows for the production overlay painter.
	const rows = Array.from({ length: Math.ceil(chars.length / 80) }, (_, i) => ({
		text: chars.slice(i * 80, (i + 1) * 80).join(""),
		isWrapped: i > 0,
	}));
	root.style.cssText =
		"position:relative;font:14px monospace;line-height:22px;background:var(--bg-primary);color:var(--fg-primary);width:100%;height:100dvh";
	const output = document.createElement("pre");
	output.style.cssText = "margin:0;font:inherit;line-height:inherit";
	output.textContent = rows.map((row) => row.text).join("\n");
	root.append(output);
	const overlay = document.createElement("div");
	overlay.style.cssText = "position:absolute;inset:0;pointer-events:none";
	root.append(overlay);
	const plan = planSuggestOverlay(rows.length, (i) => rows[i] ?? null);
	const context = document.createElement("canvas").getContext("2d")!;
	context.font = "14px monospace";
	paintOverlayBlocks(overlay, plan.blocks, 22, context.measureText("M").width);
} else render(() => <OutputView sessionId="fixture-2019" />, root);
