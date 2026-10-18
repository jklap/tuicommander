/** Real recorded Claude marker through the production mobile log renderer. */
import { render } from "solid-js/web";
import "../global.css";
import darkTheme from "../../src-tauri/src/themes/vscode-dark.json";
import lightTheme from "../../src-tauri/src/themes/vscode-light.json";
import { OutputView } from "../mobile/components/OutputView";
import fixture from "./fixtures/claude-intent-2019.json";

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
render(() => <OutputView sessionId="fixture-2019" />, root);
