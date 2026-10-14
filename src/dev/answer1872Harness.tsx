/** Dev-only recorded Rust wire frame through the real CanvasTerminal and TauriTransport.
 * Open /answer-1872-harness.html on this checkout's Vite server.
 * The frame comes from a real Codex v0.162.1 PTY capture replayed by the Rust grid.
 */
import { render } from "solid-js/web";
import "../global.css";
import "../styles.css";
import fixture from "../components/Terminal/__tests__/fixtures/codex-answer-paragraphs-1872.json";
import CanvasTerminal from "../components/Terminal/CanvasTerminal";
import { terminalsStore } from "../stores/terminals";
import "./codexIntentHarness.css";
import { Transcript } from "../components/AIChatPanel/Transcript";

const frame = Uint8Array.from(atob(fixture.frameBase64), (c) => c.charCodeAt(0)).buffer;
let deliveryCount = 0;
let channel: { onmessage: (data: ArrayBuffer) => void } | undefined;
const fixtureWindow = window as unknown as {
	__CODEX_HARNESS_INVOKE__: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
};
fixtureWindow.__CODEX_HARNESS_INVOKE__ = async (cmd, args) => {
	if (cmd === "subscribe_terminal_grid") {
		channel = args?.channel as typeof channel;
		return 1;
	}
	if (cmd === "terminal_request_frame" || cmd === "resize_pty") {
		setTimeout(() => {
			channel?.onmessage(frame.slice(0));
			document.documentElement.dataset.fixtureFrames = String(++deliveryCount);
		}, 0);
	}
	if (cmd === "resolve_terminal_paths") return [];
	if (cmd === "terminal_styled_rows") return [0, 0];
	if (cmd === "terminal_get_row_text") return fixture.rows[Number(args?.row)] ?? "";
	return null;
};

const terminalId = terminalsStore.add({
	sessionId: "fixture-codex-answer-1872",
	fontSize: 14,
	name: "Recorded Codex",
	cwd: null,
	awaitingInput: null,
});
const root = document.getElementById("answer-1872-harness");
if (!root) throw new Error("Harness mount missing");
render(
	() => (
		<main class="codex-intent-evidence">
			<header>
				<h1>Codex PTY capture · real CanvasTerminal</h1>
				<p>Recorded Codex v0.162.1 · 2026-10-13 · 100 columns</p>
			</header>
			<section class="codex-intent-evidence-terminal">
				<CanvasTerminal sessionId="fixture-codex-answer-1872" terminalId={terminalId} />
			</section>
			<section style="height: 240px; border-top: 1px solid var(--border)">
				<h2 style="padding: 8px 24px">AI Chat · same recorded answer text</h2>
				<Transcript
					entries={() => [
						{
							id: "recorded",
							kind: "agent",
							text: fixture.answerRows.map((row) => row.text.replace(/^[• ]+/, "")).join("\n"),
						},
					]}
					busy={() => false}
					emptyMessage="Empty"
					observeToolDuration={false}
				/>
			</section>
		</main>
	),
	root,
);
