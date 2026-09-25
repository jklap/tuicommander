import { For, onCleanup, onMount } from "solid-js";
import type { AgentType } from "../../agents";
import { agentConfigsStore } from "../../stores/agentConfigs";
import { answerAgentWrapPrompt, pendingAgentWrapPrompts, subscribeAgentWrapPrompt } from "../../stores/agentWrapPrompt";
import { ConfirmDialog } from "../ConfirmDialog";

interface AgentCopy {
	title: string;
	flag: string;
	purpose: string;
	/** Where the wrapper puts the flag (mirrors shell_integration.rs's zsh wrappers). */
	placement: string;
}

const AGENT_COPY: Record<string, AgentCopy> = {
	claude: {
		title: "claude",
		flag: "--settings <hooks file>",
		purpose: "lets Claude Code report busy/idle/waiting to TUIC directly",
		placement: "before your own arguments",
	},
	codex: {
		title: "codex",
		flag: "-c notify=[...]",
		purpose: "lets Codex report turn completion to TUIC directly",
		placement: "after your own arguments",
	},
	goose: {
		title: "goose",
		flag: "--name <session>",
		purpose: "keeps this tab mapped to the right Goose session",
		placement: "after the session/run subcommand",
	},
};

function messageFor(agentType: string): string {
	const copy = AGENT_COPY[agentType] ?? {
		title: agentType,
		flag: "a launch flag",
		purpose: "lets TUIC track this agent's state directly",
		placement: "to the command line",
	};
	return (
		`Your shell defines its own \`${copy.title}\` function, so TUICommander isn't adding ` +
		`${copy.flag} when you run it. That flag ${copy.purpose} — without it, TUIC has to guess ` +
		`from screen output, which is slower and less reliable.\n\n` +
		`If you allow it, TUIC will call your function with the flag added ${copy.placement}. ` +
		`This only works if your function passes its arguments through ("$@"). ` +
		`If your function already sets its own version of that flag, don't choose "Wrap my ` +
		`function" — ${copy.title} silently uses only one of them, so TUIC's could replace ` +
		`yours with no warning.\n\n` +
		`Your choice applies to this exact function: if you change it later, TUIC asks again ` +
		`before wrapping it. Applies to new terminals only. Change this later in ` +
		`Settings → Agents → ${copy.title}.`
	);
}

/**
 * Renders the "wrap my shell function?" prompt(s) a zsh tab raised — see
 * `src-tauri/src/agent_wrap_prompt.rs`'s module doc comment for the backend
 * mechanism this answers.
 *
 * Mounted by both shells (desktop and mobile), same as `McpConfirmHost` —
 * every client is shown the same request, and the first to answer wins.
 * Mobile lazy-loads it (`subscribe={false}`, see `MobileApp.tsx`).
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
export function AgentWrapPromptHost(props: { subscribe?: boolean }) {
	// An explicit answer also updates the local agentConfigsStore mirror at
	// once, so Settings → Agents reflects it without a full config reload.
	const answer = (requestId: string, agentType: string, decision: boolean | null) => {
		if (decision !== null) agentConfigsStore.syncWrapUserFunction(agentType as AgentType, decision);
		void answerAgentWrapPrompt(requestId, agentType, decision);
	};

	onMount(() => {
		// Mobile subscribes itself and mounts this lazily, only once a prompt is
		// pending (keeps the dialog + agent config store out of its entry bundle).
		if (props.subscribe === false) return;
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
					onConfirm={() => answer(request.requestId, request.agentType, true)}
					onDiscard={() => answer(request.requestId, request.agentType, false)}
					onClose={() => answer(request.requestId, request.agentType, null)}
				/>
			)}
		</For>
	);
}

export default AgentWrapPromptHost;
