/** Dev-only browser verification against an isolated, real tuic-remote daemon. */

// Side-effect only: the desktop-only COMMAND_TABLE entries (see
// transportExtended.ts) — this page renders desktop components in browser mode.
import "../transportExtended";
import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import CanvasTerminal from "../components/Terminal/CanvasTerminal";
import { ToastContainer } from "../components/ToastContainer/ToastContainer";
import { terminalsStore } from "../stores/terminals";
import { toastsStore } from "../stores/toasts";
import { rpc } from "../transport";
import { setRemoteBaseUrlLookup, setRemoteTokenLookup, setSessionConnectionLookup } from "../transportRuntime";
import { sendCommand } from "../utils/sendCommand";
import "../global.css";

const params = new URLSearchParams(location.search);
const base = `http://127.0.0.1:${params.get("port") ?? "19877"}`;
const requestedSession = params.get("session");
if (!requestedSession) throw new Error("Pass the disposable fixture session as ?session=<id>&port=<port>");
const sessionId = requestedSession;
const token = "tuic-1421-fixture";
setRemoteBaseUrlLookup(() => base);
setRemoteTokenLookup(() => token);
setSessionConnectionLookup((id) => (id === sessionId ? "fixture" : undefined));
const terminalId = terminalsStore.add({
	sessionId,
	cwd: null,
	name: "Remote replay fixture",
	fontSize: 13,
	awaitingInput: null,
});
const [attached, setAttached] = createSignal(false);
const [message, setMessage] = createSignal("Prepare existing scrollback, then attach the renderer.");

async function prepare(): Promise<void> {
	try {
		await sendCommand(
			(data) => rpc<void>("write_pty", { id: sessionId, data }, "fixture"),
			"i=1; while [ \"$i\" -le 1150 ]; do printf 'REMOTE REPLAY %s\\n' \"$i\"; i=$((i+1)); done; printf 'READY FROM EXISTING SCROLLBACK\\n'",
		);
		setMessage("Scrollback prepared. Attach after the command has finished.");
	} catch (error) {
		toastsStore.add("Fixture setup failed", String(error), "error");
	}
}

const root = document.getElementById("fixture");
if (!root) throw new Error("Fixture mount point is missing");

render(
	() => (
		<main
			style={{
				height: "100vh",
				display: "flex",
				"flex-direction": "column",
				background: "var(--bg-primary)",
				color: "var(--fg-primary)",
			}}
		>
			<header style={{ padding: "8px", display: "flex", gap: "12px", "align-items": "center" }}>
				<button onClick={() => void prepare()}>Prepare existing scrollback</button>
				<button onClick={() => setAttached(true)}>Attach remote terminal</button>
				<span>{message()}</span>
			</header>
			<section style={{ flex: "1", "min-height": "0", position: "relative" }}>
				<Show when={attached()}>
					<CanvasTerminal sessionId={sessionId} terminalId={terminalId} />
				</Show>
			</section>
			<ToastContainer />
		</main>
	),
	root,
);
