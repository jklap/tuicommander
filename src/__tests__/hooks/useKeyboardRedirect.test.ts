import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

// Use vi.hoisted so these are available when the mock factory runs (vi.mock is hoisted)
const { mockWrite, mockFocus, mockGetActive } = vi.hoisted(() => ({
	mockWrite: vi.fn(),
	mockFocus: vi.fn(),
	mockGetActive: vi.fn(),
}));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		getActive: mockGetActive,
	},
}));

import { createRoot } from "solid-js";
import { useKeyboardRedirect } from "../../hooks/useKeyboardRedirect";
import { __resetModalStackForTest, popModal, pushModal } from "../../stores/modalStack";
import { testInScopeAsync } from "../helpers/store";

/** Dispatch a keydown event on document */
function dispatchKeydown(key: string, opts: Partial<KeyboardEvent> = {}): void {
	const event = new KeyboardEvent("keydown", {
		key,
		bubbles: true,
		cancelable: true,
		...opts,
	});
	document.dispatchEvent(event);
}

/** Flush SolidJS effects (createEffect uses queueMicrotask internally) */
function flushEffects(): Promise<void> {
	return new Promise((resolve) => queueMicrotask(resolve));
}

describe("useKeyboardRedirect", () => {
	beforeEach(() => {
		mockWrite.mockReset();
		mockFocus.mockReset();
		mockGetActive.mockReset();
		// Default: active terminal exists with ref
		mockGetActive.mockReturnValue({
			id: "term-1",
			ref: {
				write: mockWrite,
				focus: mockFocus,
				fit: vi.fn(),
				writeln: vi.fn(),
				clear: vi.fn(),
				getSessionId: vi.fn(),
				openSearch: vi.fn(),
				closeSearch: vi.fn(),
				searchBuffer: vi.fn(() => []),
				scrollToLine: vi.fn(),
				scrollToTop: vi.fn(),
				scrollToBottom: vi.fn(),
				scrollPages: vi.fn(),
			},
		});
	});

	afterEach(() => {
		// Ensure no DOM state leaks between tests
		if (document.activeElement instanceof HTMLElement) {
			document.activeElement.blur();
		}
	});

	describe("printable character redirect", () => {
		it("redirects a printable character to the active terminal", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("a");

				expect(mockWrite).toHaveBeenCalledWith("a");
				expect(mockFocus).toHaveBeenCalled();
			});
		});

		it("redirects uppercase letters", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("Z");

				expect(mockWrite).toHaveBeenCalledWith("Z");
			});
		});

		it("redirects space character", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown(" ");

				expect(mockWrite).toHaveBeenCalledWith(" ");
			});
		});
	});

	describe("special keys", () => {
		it("redirects Backspace as DEL character", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("Backspace");

				expect(mockWrite).toHaveBeenCalledWith("\x7f");
			});
		});

		it("redirects Delete as escape sequence", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("Delete");

				expect(mockWrite).toHaveBeenCalledWith("\x1b[3~");
			});
		});
	});

	describe("excluded keys", () => {
		it("does not redirect Tab", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("Tab");

				expect(mockWrite).not.toHaveBeenCalled();
			});
		});

		it("does not redirect Escape", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("Escape");

				expect(mockWrite).not.toHaveBeenCalled();
			});
		});

		it("does not redirect arrow keys", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("ArrowUp");
				dispatchKeydown("ArrowDown");
				dispatchKeydown("ArrowLeft");
				dispatchKeydown("ArrowRight");

				expect(mockWrite).not.toHaveBeenCalled();
			});
		});

		it("does not redirect function keys", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("F1");
				dispatchKeydown("F12");

				expect(mockWrite).not.toHaveBeenCalled();
			});
		});

		it("does not redirect Enter", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("Enter");

				expect(mockWrite).not.toHaveBeenCalled();
			});
		});
	});

	describe("modifier keys", () => {
		it("does not redirect when Ctrl is held", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("c", { ctrlKey: true });

				expect(mockWrite).not.toHaveBeenCalled();
			});
		});

		it("does not redirect when Meta/Cmd is held", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("v", { metaKey: true });

				expect(mockWrite).not.toHaveBeenCalled();
			});
		});

		it("does not redirect when Alt is held", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("x", { altKey: true });

				expect(mockWrite).not.toHaveBeenCalled();
			});
		});
	});

	describe("focus context", () => {
		it("does not redirect when focus is on an input element", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				const input = document.createElement("input");
				document.body.appendChild(input);
				input.focus();

				dispatchKeydown("a");

				expect(mockWrite).not.toHaveBeenCalled();

				document.body.removeChild(input);
			});
		});

		it("does not redirect when focus is on a textarea", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				const textarea = document.createElement("textarea");
				document.body.appendChild(textarea);
				textarea.focus();

				dispatchKeydown("b");

				expect(mockWrite).not.toHaveBeenCalled();

				document.body.removeChild(textarea);
			});
		});

		it("does not redirect when focus is on a select element", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				const select = document.createElement("select");
				document.body.appendChild(select);
				select.focus();

				dispatchKeydown("c");

				expect(mockWrite).not.toHaveBeenCalled();

				document.body.removeChild(select);
			});
		});

		it("does not redirect when focus is inside a terminal pane", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				const terminalPane = document.createElement("div");
				terminalPane.classList.add("terminal-pane");
				const child = document.createElement("div");
				child.setAttribute("tabindex", "0");
				terminalPane.appendChild(child);
				document.body.appendChild(terminalPane);
				child.focus();

				dispatchKeydown("d");

				expect(mockWrite).not.toHaveBeenCalled();

				document.body.removeChild(terminalPane);
			});
		});

		it("does not redirect when focus is inside an xterm element", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				const xterm = document.createElement("div");
				xterm.classList.add("xterm");
				const child = document.createElement("div");
				child.setAttribute("tabindex", "0");
				xterm.appendChild(child);
				document.body.appendChild(xterm);
				child.focus();

				dispatchKeydown("e");

				expect(mockWrite).not.toHaveBeenCalled();

				document.body.removeChild(xterm);
			});
		});
	});

	describe("modal/dialog open", () => {
		afterEach(() => {
			__resetModalStackForTest();
		});

		it("does not redirect a printable key while a modal is registered, even with a focused button", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				// Regression: the New Worktree dialog's "Start from" trigger is a
				// real <button>, not an INPUT_ELEMENTS member, so without a modal
				// check this key would previously reach the terminal.
				const button = document.createElement("button");
				document.body.appendChild(button);
				button.focus();
				pushModal(() => {});

				dispatchKeydown("x");

				expect(mockWrite).not.toHaveBeenCalled();
				expect(mockFocus).not.toHaveBeenCalled();

				document.body.removeChild(button);
			});
		});

		it("still redirects the same focused-button case once no modal is open", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				const button = document.createElement("button");
				document.body.appendChild(button);
				button.focus();

				dispatchKeydown("x");

				expect(mockWrite).toHaveBeenCalledWith("x");

				document.body.removeChild(button);
			});
		});

		it("popModal restores redirect behavior", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				const button = document.createElement("button");
				document.body.appendChild(button);
				button.focus();
				const id = pushModal(() => {});
				popModal(id);

				dispatchKeydown("z");

				expect(mockWrite).toHaveBeenCalledWith("z");

				document.body.removeChild(button);
			});
		});
	});

	describe("no active terminal", () => {
		it("does not write when there is no active terminal", async () => {
			await testInScopeAsync(async () => {
				mockGetActive.mockReturnValue(undefined);
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("a");

				expect(mockWrite).not.toHaveBeenCalled();
			});
		});

		it("does not write when active terminal has no ref", async () => {
			await testInScopeAsync(async () => {
				mockGetActive.mockReturnValue({ id: "term-1", ref: undefined });
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("a");

				expect(mockWrite).not.toHaveBeenCalled();
			});
		});
	});

	describe("autoFocus parameter", () => {
		it("does not focus terminal when autoFocus is false", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect(false);
				await flushEffects();

				dispatchKeydown("a");

				expect(mockWrite).toHaveBeenCalledWith("a");
				expect(mockFocus).not.toHaveBeenCalled();
			});
		});
	});

	describe("digit keys", () => {
		// Baseline: a bare digit is a printable single character, so with nothing else
		// listening it reaches the PTY exactly like a letter would. This is the specific
		// hazard an overlay's "press 1-9 to jump" hotkey must guard against — see the next
		// block.
		it("redirects a bare digit when nothing intercepts it", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				dispatchKeydown("1");

				expect(mockWrite).toHaveBeenCalledWith("1");
			});
		});
	});

	describe("interception by a capture-phase listener", () => {
		// useKeyboardRedirect listens on `document` at the BUBBLE phase (the hook's
		// document.addEventListener call passes no `useCapture` flag). An overlay that
		// wants to consume a key (e.g. digit-to-jump) before it reaches the PTY must
		// register a document-level CAPTURE listener and call stopPropagation — the same
		// pattern stores/modalStack.ts uses for Escape.
		//
		// The event must be dispatched on a DESCENDANT of document, not on document
		// itself: per the DOM spec, listeners registered on the event's own target fire in
		// registration order regardless of capture/bubble phase, so dispatching on
		// `document` would make this test pass even if capture-ordering were broken.
		it("capture-phase stopPropagation prevents the redirect from ever seeing the key", async () => {
			await testInScopeAsync(async () => {
				useKeyboardRedirect();
				await flushEffects();

				const target = document.createElement("div");
				document.body.appendChild(target);

				const intercept = (e: KeyboardEvent) => {
					e.preventDefault();
					e.stopPropagation();
				};
				document.addEventListener("keydown", intercept, true);

				const event = new KeyboardEvent("keydown", { key: "1", bubbles: true, cancelable: true });
				target.dispatchEvent(event);

				expect(mockWrite).not.toHaveBeenCalled();

				document.removeEventListener("keydown", intercept, true);
				target.remove();
			});
		});
	});

	describe("cleanup", () => {
		it("removes event listener on dispose", async () => {
			// Raw createRoot: we need to call dispose() mid-test to verify cleanup
			await createRoot(async (dispose) => {
				useKeyboardRedirect();
				await flushEffects();

				// Verify it works before dispose
				dispatchKeydown("a");
				expect(mockWrite).toHaveBeenCalledTimes(1);

				dispose();

				// After dispose, should not redirect
				mockWrite.mockReset();
				dispatchKeydown("b");
				expect(mockWrite).not.toHaveBeenCalled();
			});
		});
	});
});
