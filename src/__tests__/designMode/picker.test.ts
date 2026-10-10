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
	const extract = vi.fn(() => ({ selector: "#browse" }));
	new Function("chrome", "tuicExtract", readFileSync("browser-extension/design-mode/picker.js", "utf8"))(
		chrome,
		extract,
	);
	const button = document.createElement("button");
	button.id = "browse";
	document.body.append(button);
	const browse = vi.fn();
	button.addEventListener("click", browse);
	// happy-dom has no browser input driver. Mark DOM-dispatched events trusted
	// to exercise the same capture handler used for browser input.
	const click = () => {
		const event = new MouseEvent("click", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "isTrusted", { value: true });
		button.dispatchEvent(event);
		return event;
	};
	const assertBrowsing = () => {
		browse.mockClear();
		extract.mockClear();
		expect(click().defaultPrevented).toBe(false);
		expect(browse).toHaveBeenCalledOnce();
		expect(extract).not.toHaveBeenCalled();
	};
	assertBrowsing();
	for (const cancel of ["escape", "stop", "timeout", "pagehide"]) {
		receive({ kind: "arm", multi: true });
		browse.mockClear();
		expect(click().defaultPrevented).toBe(true);
		expect(browse).not.toHaveBeenCalled();
		expect(extract).toHaveBeenCalled();
		if (cancel === "escape") {
			window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
		} else if (cancel === "stop") {
			receive({ kind: "stop" });
		} else if (cancel === "timeout") {
			vi.advanceTimersByTime(120000);
		} else {
			window.dispatchEvent(new Event("pagehide"));
		}
		assertBrowsing();
	}
	button.remove();
	vi.useRealTimers();
});
