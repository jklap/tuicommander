import { describe, expect, it } from "vitest";

import { keyToSequence } from "../terminalInput";

/**
 * Regression coverage for the "cursor caught before the last character" bug.
 *
 * zsh's zle enables DECCKM (application cursor keys) on every prompt via `smkx`.
 * Under DECCKM, unmodified arrows/Home/End must be sent as SS3 (`\x1bO...`)
 * instead of CSI (`\x1b[...]`). A terminal that keeps sending CSI can, under
 * `bindkey -v`, land an unbound sequence's leading ESC on `vi-cmd-mode` —
 * dropping the user into vi normal mode with no visual cue.
 */
function key(key: string, opts: Partial<KeyboardEventInit> = {}): KeyboardEvent {
	return new KeyboardEvent("keydown", { key, ...opts });
}

describe("keyToSequence — DECCKM off (default)", () => {
	it("sends CSI for unmodified arrows", () => {
		expect(keyToSequence(key("ArrowRight"))).toBe("\x1b[C");
		expect(keyToSequence(key("ArrowLeft"))).toBe("\x1b[D");
		expect(keyToSequence(key("ArrowUp"))).toBe("\x1b[A");
		expect(keyToSequence(key("ArrowDown"))).toBe("\x1b[B");
	});

	it("sends CSI for Home/End", () => {
		expect(keyToSequence(key("Home"))).toBe("\x1b[H");
		expect(keyToSequence(key("End"))).toBe("\x1b[F");
	});
});

describe("keyToSequence — DECCKM on (appCursor=true)", () => {
	it("sends SS3 for unmodified arrows", () => {
		expect(keyToSequence(key("ArrowRight"), true)).toBe("\x1bOC");
		expect(keyToSequence(key("ArrowLeft"), true)).toBe("\x1bOD");
		expect(keyToSequence(key("ArrowUp"), true)).toBe("\x1bOA");
		expect(keyToSequence(key("ArrowDown"), true)).toBe("\x1bOB");
	});

	it("sends SS3 for Home/End — the exact bug: zsh's viins/vicmd bind ^[OH/^[OF, not ^[[H/^[[F", () => {
		expect(keyToSequence(key("End"), true)).toBe("\x1bOF");
		expect(keyToSequence(key("Home"), true)).toBe("\x1bOH");
	});

	it("keeps modified arrows as CSI — DECCKM only changes the unmodified form", () => {
		expect(keyToSequence(key("ArrowRight", { ctrlKey: true }), true)).toBe("\x1b[1;5C");
		expect(keyToSequence(key("ArrowLeft", { altKey: true }), true)).toBe("\x1b[1;3D");
		expect(keyToSequence(key("ArrowUp", { shiftKey: true }), true)).toBe("\x1b[1;2A");
	});

	it("leaves CSI-tilde nav keys unaffected", () => {
		expect(keyToSequence(key("Delete"), true)).toBe("\x1b[3~");
		expect(keyToSequence(key("Insert"), true)).toBe("\x1b[2~");
		expect(keyToSequence(key("PageUp"), true)).toBe("\x1b[5~");
		expect(keyToSequence(key("PageDown"), true)).toBe("\x1b[6~");
	});

	it("leaves function keys unaffected", () => {
		expect(keyToSequence(key("F1"), true)).toBe("\x1bOP");
		expect(keyToSequence(key("F5"), true)).toBe("\x1b[15~");
	});
});
