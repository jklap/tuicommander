/** Dev-only recorded Rust wire frame through the real CanvasTerminal and TauriTransport.
 * Open /ego-intent-harness.html on this checkout's Vite server.
 * The frame comes from the unchanged Ego PTY capture, not hand-written rows.
 */
import { render } from "solid-js/web";
import "../global.css";
import "../styles.css";
import { Show } from "solid-js";
import darkTheme from "../../src-tauri/src/themes/vscode-dark.json";
import lightTheme from "../../src-tauri/src/themes/vscode-light.json";
import { TerminalTabView } from "../components/TabBar/TabViews";
import CanvasTerminal from "../components/Terminal/CanvasTerminal";
import { handleIntentEvent } from "../components/Terminal/intentTitle";
import { LastPromptBar } from "../components/Terminal/LastPromptBar";
import { INTENT_HIGHLIGHT_RE } from "../components/Terminal/suggestOverlay";
import { terminalsStore } from "../stores/terminals";
import originalFixture from "./fixtures/ego-intent-1948.json";
import wrappedFixture from "./fixtures/ego-intent-1949.json";
import "./codexIntentHarness.css";

const params = new URLSearchParams(location.search);
const fixture = ["1949", "1964"].includes(params.get("fixture") ?? "") ? wrappedFixture : originalFixture;
// Use shipped palette values without loading or saving live application settings.
const theme = params.get("theme") === "light" ? lightTheme : darkTheme;
for (const [key, value] of Object.entries({
	"--bg-primary": theme.appChrome.background,
	"--bg-secondary": theme.appChrome.surface,
	"--bg-tertiary": theme.appChrome.surfaceElevated,
	"--fg-primary": theme.appChrome.foreground,
	"--fg-secondary": theme.appChrome.foregroundSecondary,
	"--border": theme.appChrome.border,
	"--warning": theme.appChrome.warning,
})) {
	document.documentElement.style.setProperty(key, value);
}

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
	sessionId: "fixture-codex-intent-1858",
	fontSize: 14,
	name: "Recorded ego",
	cwd: null,
	awaitingInput: null,
});
// This mode verifies the real UI consumers, not backend parser execution.
// The backend regression separately replays ego-intent-1964.tcap.
const showConsumers = params.get("fixture") === "1964";
if (showConsumers) {
	terminalsStore.update(terminalId, { agentType: "ego" });
	handleIntentEvent({
		terminalId,
		text: "Previous task",
		title: "Previous task",
		globalEnabled: true,
		perAgentEnabled: true,
	});
	const marker = fixture.rows
		.slice(0, 2)
		.map((row) => row.trimEnd())
		.join(" ");
	const body = marker.replace(INTENT_HIGHLIGHT_RE, "");
	const parsed = /^(.*?)\s*\(([^)]+)\)$/.exec(body);
	if (!parsed) throw new Error("Recorded intent title missing");
	handleIntentEvent({ terminalId, text: parsed[1], title: parsed[2], globalEnabled: true, perAgentEnabled: true });
}
const root = document.getElementById("codex-intent-harness");
if (!root) throw new Error("Harness mount missing");
render(
	() => (
		<main class="codex-intent-evidence">
			<header>
				<h1>Ego PTY capture · real CanvasTerminal</h1>
				<p>
					Recorded 2026-10-16 · Rust wire frame at record {fixture.source.absoluteRows[0]} · {80} columns
				</p>
			</header>
			<Show when={showConsumers}>
				<p>
					UI consumer evidence: second intent from recorded ego cells; backend replay is a separate Rust regression.
				</p>
				<div style={{ display: "flex" }}>
					<TerminalTabView
						id={terminalId}
						index={0}
						paneRects={[]}
						isDragging={false}
						isDragOver={false}
						dragOverSide={null}
						dragInvalid={false}
						quickSwitcherActive={false}
						isEditing={false}
						showWorkspaceMetadata={false}
						onSelect={() => {}}
						onClose={() => {}}
						onContextMenu={() => {}}
						onPointerDown={() => {}}
						onStartEditing={() => {}}
						onCommitRename={() => {}}
					/>
				</div>
				<LastPromptBar
					intent={() => terminalsStore.get(terminalId)?.agentIntent ?? null}
					ptyDescription={() => null}
					prompt={() => null}
				/>
			</Show>
			<section class="codex-intent-evidence-terminal">
				<CanvasTerminal sessionId="fixture-codex-intent-1858" terminalId={terminalId} />
			</section>
		</main>
	),
	root,
);
