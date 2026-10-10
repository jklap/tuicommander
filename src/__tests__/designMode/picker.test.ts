import { readFileSync } from "node:fs";
import { expect, it, vi } from "vitest";

// Catches: the overlay intercepting normal browsing before arm or after Escape/Stop/timeout.
it("leaves browsing alone unless explicitly armed and removes the overlay on every cancellation", () => {
	vi.useFakeTimers();
	let receive: (message: { kind: string; multi?: boolean }) => void = () => {};
	const chrome = {
		runtime: {
			sendMessage: vi.fn(async () => {}),
			onMessage: {
				addListener: (listener: typeof receive) => {
					receive = listener;
				},
			},
		},
	};
	new Function("chrome", "tuicExtract", readFileSync("browser-extension/design-mode/picker.js", "utf8"))(
		chrome,
		vi.fn(),
	);
	const children = document.documentElement.children.length;
	const click = new MouseEvent("click", { bubbles: true, cancelable: true });
	document.body.dispatchEvent(click);
	expect(click.defaultPrevented).toBe(false);
	expect(document.documentElement.children).toHaveLength(children);
	receive({ kind: "arm" });
	expect(document.documentElement.children).toHaveLength(children + 1);
	const escapeEvent = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
	window.dispatchEvent(escapeEvent);
	expect(escapeEvent.defaultPrevented).toBe(true);
	expect(document.documentElement.children).toHaveLength(children);
	receive({ kind: "arm", multi: true });
	receive({ kind: "stop" });
	expect(document.documentElement.children).toHaveLength(children);
	receive({ kind: "arm" });
	vi.advanceTimersByTime(120000);
	expect(document.documentElement.children).toHaveLength(children);
	window.dispatchEvent(new Event("pagehide"));
	vi.useRealTimers();
});
