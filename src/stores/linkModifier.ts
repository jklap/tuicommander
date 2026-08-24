import { createSignal } from "solid-js";
import { isLinkModifier } from "../platform";

/**
 * Tracks whether the terminal link-activation modifier (Cmd on macOS, Ctrl on
 * Windows/Linux) is currently held down. Backs "modifier" link-activation
 * mode: links stay underlined and clickable only while this is true.
 *
 * A single module-level signal fed by document-level listeners, shared by
 * every terminal pane — one set of listeners regardless of pane count.
 */
const [linkModifierHeldSignal, setLinkModifierHeld] = createSignal(false);

export const linkModifierHeld = linkModifierHeldSignal;

function updateFromEvent(e: KeyboardEvent) {
	const held = isLinkModifier(e);
	if (held !== linkModifierHeldSignal()) setLinkModifierHeld(held);
}

function reset() {
	setLinkModifierHeld(false);
}

let initialised = false;

/**
 * Register document-level keydown/keyup listeners. Idempotent — safe to call
 * from every CanvasTerminal onMount.
 *
 * Resets on both `blur` and `visibilitychange`: a held-modifier flag can get
 * stuck if focus leaves mid-hold (e.g. Cmd+Tab away while holding Cmd) and
 * either event alone can miss that transition.
 */
export function initLinkModifier(): void {
	if (initialised) return;
	initialised = true;

	document.addEventListener("keydown", updateFromEvent);
	document.addEventListener("keyup", updateFromEvent);
	window.addEventListener("blur", reset);
	document.addEventListener("visibilitychange", reset);
}
