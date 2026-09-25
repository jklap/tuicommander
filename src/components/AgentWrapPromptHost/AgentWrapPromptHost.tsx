import { For, onCleanup, onMount } from "solid-js";
import { answerAgentWrapPrompt, pendingAgentWrapPrompts, subscribeAgentWrapPrompt } from "../../stores/agentWrapPrompt";
import { ConfirmDialog } from "../ConfirmDialog";

interface AgentCopy {
	title: string;
	flag: string;
	purpose: string;
}

const AGENT_COPY: Record<string, AgentCopy> = {
	claude: {
		title: "claude",
		flag: "--settings <hooks file>",
		purpose: "lets Claude Code report busy/idle/waiting to TUIC directly",
	},
	codex: {
		title: "codex",
		flag: "-c notify=[...]",
		purpose: "lets Codex report turn completion to TUIC directly",
	},
	goose: {
		title: "goose",
		flag: "--name <session>",
		purpose: "keeps this tab mapped to the right Goose session",
	},
};

function messageFor(agentType: string): string {
	const copy = AGENT_COPY[agentType] ?? {
		title: agentType,
		flag: "a launch flag",
		purpose: "lets TUIC track this agent's state directly",
	};
	return (
		`Your shell defines its own \`${copy.title}\` function, so TUICommander isn't adding ` +
		`${copy.flag} when you run it. That flag ${copy.purpose} — without it, TUIC has to guess ` +
		`from screen output, which is slower and less reliable.\n\n` +
		`If you allow it, TUIC will call your function and append the flag after your own ` +
		`arguments. This only works if your function passes its arguments through ("$@"). ` +
		`If your function already sets its own version of that flag, don't choose "Wrap my ` +
		`function" — ${copy.title} silently uses only the last one, so TUIC's would replace ` +
		`yours with no warning.\n\n` +
		`Applies to new terminals only. Change this later in Settings → Agents → ${copy.title}.`
	);
}

/**
 * Renders the "wrap my shell function?" prompt(s) a zsh tab raised — see
 * `src-tauri/src/agent_wrap_prompt.rs`'s module doc comment for the backend
 * mechanism this answers.
 *
 * Mounted by both shells (desktop and mobile), same as `McpConfirmHost` —
 * every client is shown the same request, and the first to answer wins.
 *
 * Deliberately NOT built on `McpConfirmHost`: this needs three outcomes
 * (wrap / leave alone / not now), not two, and isn't a destructive-action
 * confirmation — `ConfirmDialog`'s existing `discardLabel`/`onDiscard`
 * middle-button support already covers the three-way shape without needing
 * a bespoke dialog.
 *
 * Renders one dialog per agent with a pending prompt — claude/codex/goose
 * can each have their own prompt in flight at once (unlike `McpConfirmHost`,
 * which shows one ordered queue).
 */
export function AgentWrapPromptHost() {
	onMount(() => {
		const unsubscribe = subscribeAgentWrapPrompt();
		onCleanup(() => {
			unsubscribe.then((fn) => fn()).catch(() => {});
		});
	});

	return (
		<For each={pendingAgentWrapPrompts()}>
			{(request) => (
				<ConfirmDialog
					visible={true}
					title={`Wrap your ${AGENT_COPY[request.agentType]?.title ?? request.agentType} function?`}
					message={messageFor(request.agentType)}
					confirmLabel="Wrap my function"
					discardLabel="Leave it alone"
					cancelLabel="Not now"
					kind="info"
					// Neither answer is destructive/risky enough to force Enter
					// away from it the way McpConfirmHost does, but this dialog
					// carries a caveat worth reading first, so Enter still
					// defaults to the no-op ("Not now") rather than silently
					// opting in.
					defaultButton="cancel"
					onConfirm={() => void answerAgentWrapPrompt(request.requestId, request.agentType, true)}
					onDiscard={() => void answerAgentWrapPrompt(request.requestId, request.agentType, false)}
					onClose={() => void answerAgentWrapPrompt(request.requestId, request.agentType, null)}
				/>
			)}
		</For>
	);
}

export default AgentWrapPromptHost;
