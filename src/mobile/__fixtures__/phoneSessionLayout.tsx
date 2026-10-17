/** Synthetic session identity for layout capture; not agent/transport evidence. */
import { render } from "solid-js/web";
import commander from "../../../src-tauri/src/themes/commander.json";
import paper from "../../../src-tauri/src/themes/vscode-light.json";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import "../mobile.css";

const light = new URLSearchParams(location.search).get("theme") === "light";
const names: Record<string, string> = {
	background: "bg-primary",
	surface: "bg-secondary",
	surfaceElevated: "bg-tertiary",
	highlight: "bg-highlight",
	foreground: "fg-primary",
	foregroundSecondary: "fg-secondary",
	mutedForeground: "fg-muted",
	accentForeground: "text-on-accent",
	errorForeground: "text-on-error",
	successForeground: "text-on-success",
};
for (const [key, value] of Object.entries((light ? paper : commander).appChrome)) {
	document.documentElement.style.setProperty(
		`--${names[key] ?? key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`,
		value,
	);
}
document.documentElement.style.colorScheme = light ? "light" : "dark";
render(
	() => (
		<SessionDetailScreen
			session={{
				session_id: "layout-only",
				display_name: "COORDINATOR",
				cwd: "/layout/orchestrator",
				worktree_path: null,
				worktree_branch: null,
				state: {
					agent_type: "claude",
					awaiting_input: false,
					rate_limited: false,
					last_activity_ms: 0,
					active_sub_tasks: 3,
				},
			}}
			subscribeChatWake={() => () => {}}
			sessionExists
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	),
	document.getElementById("mobile-app")!,
);
