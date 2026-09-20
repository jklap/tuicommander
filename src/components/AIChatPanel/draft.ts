import { createSignal } from "solid-js";

/**
 * What the person has typed but not sent yet.
 *
 * Module scope rather than component state so the panel can be closed and
 * reopened without losing a half-written question, and so something outside the
 * panel — the terminal context menu — can hand it a selection to ask about.
 */
const [text, setText] = createSignal("");

export const aiChatDraft = {
	text,
	set: setText,

	/** Add text to the draft and leave the cursor after it. */
	append(addition: string): void {
		const current = text();
		setText(current ? `${current.replace(/\s+$/, "")}\n\n${addition}` : addition);
	},

	clear(): void {
		setText("");
	},
};
