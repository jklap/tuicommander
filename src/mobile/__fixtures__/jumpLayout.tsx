/** Real mobile components with inert transport: geometry, not session evidence. */
import { render } from "solid-js/web";
import { CommandInput } from "../components/CommandInput";
import { TerminalKeybar } from "../components/TerminalKeybar";
import "../mobile.css";

render(
	() => (
		<section style={{ display: "flex", "flex-direction": "column", height: "100dvh" }}>
			<div style={{ flex: "1", padding: "12px" }}>Earlier terminal output</div>
			<TerminalKeybar sessionId="layout-only" sessionExists={false} />
			<CommandInput sessionId="layout-only" sessionExists={false} showScrollToBottom />
		</section>
	),
	document.getElementById("mobile-app")!,
);
