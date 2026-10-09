/**
 * "Explain with AI" and "Fix this error", on terminal right-click.
 *
 * Both entries keep the ids and labels they had before the embedded engine went
 * away, because a person's muscle memory is part of the contract. What changed
 * is where the text goes: the panel is bound to a repository and a session, not
 * to the terminal that was right-clicked, so these hand it a question about some
 * output rather than attaching the conversation to that terminal.
 *
 * The question is put in the composer instead of being sent. ego is launched
 * lazily by the panel, so "send" at this moment would either race a connection
 * that is still coming up or need a queue of its own; leaving the draft ready
 * costs one keystroke and cannot fail.
 */

import { appLogger } from "../../stores/appLogger";
import { contextMenuActionsStore } from "../../stores/contextMenuActionsStore";
import { settingsStore } from "../../stores/settings";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { uiStore } from "../../stores/ui";
import { aiChatDraft } from "./draft";

const PLUGIN_ID = "ai-chat";
const MAX_CHARS = 2000;

/** Cut text to `maxChars`, saying so where it was cut. */
export function truncateText(text: string, maxChars: number): string {
	if (text.length <= maxChars) return text;
	return `${text.slice(0, maxChars)}\n[... truncated]`;
}

/**
 * The output to ask about: what is selected, or the tail of the screen.
 *
 * The selection comes off the terminal rather than off the document: the grid
 * is drawn on a canvas, so a highlighted region is not a DOM selection and
 * `window.getSelection()` returns nothing for it.
 */
async function terminalText(sessionId?: string): Promise<string> {
	if (!sessionId) return "";
	const entry = Object.values(terminalsStore.state.terminals).find((terminal) => terminal.sessionId === sessionId);
	const selected = entry?.ref?.getSelection().trim();
	if (selected) return selected;
	if (!entry?.ref) return "";
	try {
		const lines = await entry.ref.getBufferLines(0, 999_999);
		return lines.slice(-50).join("\n").trimEnd();
	} catch (error) {
		appLogger.warn("ai-chat", "Failed to read terminal buffer", { error: String(error) });
		return "";
	}
}

/** Open the panel with a question about `text` already written. */
async function ask(sessionId: string | undefined, question: (output: string) => string): Promise<void> {
	const raw = await terminalText(sessionId);
	if (!raw) {
		appLogger.info("ai-chat", "No terminal text to ask about");
		return;
	}
	aiChatDraft.append(question(truncateText(raw, MAX_CHARS)));
	uiStore.setAiChatPanelVisible(true);
}

/**
 * Smart Selection's "Ask AI" action: draft `text` into the AI Chat composer and
 * open the panel — but only while AI Chat is enabled. With the experimental
 * switch off the panel never renders (`PanelOrchestrator`), so drafting and
 * flipping `aiChatPanelVisible` would do nothing visible and still persist the
 * flag; say why instead. Returns whether the text was drafted.
 */
export function askAiAboutText(text: string): boolean {
	if (!settingsStore.isAiChatEnabled()) {
		toastsStore.add("AI Chat is disabled", "Enable AI Chat in Settings to use Ask AI", "warn");
		return false;
	}
	aiChatDraft.append(text);
	uiStore.setAiChatPanelVisible(true);
	return true;
}

export function registerAiChatContextActions(): Array<{ dispose(): void }> {
	return [
		contextMenuActionsStore.registerContextAction(PLUGIN_ID, {
			id: "ai-chat:explain",
			label: "Explain with AI",
			target: "terminal",
			action: (context) => {
				void ask(context.sessionId, (output) => `Explain this terminal output:\n\n\`\`\`\n${output}\n\`\`\``);
			},
		}),
		contextMenuActionsStore.registerContextAction(PLUGIN_ID, {
			id: "ai-chat:fix-error",
			label: "Fix this error",
			target: "terminal",
			action: (context) => {
				void ask(
					context.sessionId,
					(output) =>
						`Analyze this terminal error and suggest a fix:\n\n\`\`\`\n${output}\n\`\`\`\n\n` +
						"Explain: 1) What went wrong 2) The root cause 3) How to fix it",
				);
			},
		}),
	];
}
