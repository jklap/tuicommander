import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { appLogger } from "../../stores/appLogger";
import { copyPathToClipboard, readClipboard, writeClipboard, writeClipboardAsync } from "../../utils/clipboard";
import { FakeClipboardItem } from "../mocks/clipboardItem";
import { mockInvoke } from "../mocks/tauri";

/** setup.ts sets __TAURI_INTERNALS__ globally so every other suite defaults to
 *  Tauri mode; these tests need to flip to browser mode for some cases. */
function setTauriMode(enabled: boolean) {
	if (enabled) {
		(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
	} else {
		delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
	}
}

describe("clipboard", () => {
	afterEach(() => {
		setTauriMode(true);
		vi.restoreAllMocks();
	});

	describe("Tauri mode", () => {
		beforeEach(() => {
			setTauriMode(true);
			mockInvoke.mockReset();
		});

		it("writeClipboard routes through the native clipboard-manager plugin", async () => {
			mockInvoke.mockResolvedValue(undefined);

			await writeClipboard("hello");

			expect(mockInvoke).toHaveBeenCalledWith("plugin:clipboard-manager|write_text", {
				text: "hello",
				label: undefined,
			});
		});

		it("readClipboard routes through the native clipboard-manager plugin", async () => {
			mockInvoke.mockResolvedValue("clipboard text");

			await expect(readClipboard()).resolves.toBe("clipboard text");
			expect(mockInvoke).toHaveBeenCalledWith("plugin:clipboard-manager|read_text");
		});

		it("writeClipboard propagates native plugin failures without falling back", async () => {
			mockInvoke.mockRejectedValue(new Error("native clipboard error"));

			await expect(writeClipboard("hello")).rejects.toThrow("native clipboard error");
		});
	});

	describe("browser mode", () => {
		beforeEach(() => {
			setTauriMode(false);
		});

		it("writeClipboard uses navigator.clipboard.writeText when it succeeds", async () => {
			const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue(undefined);

			await writeClipboard("hello");

			expect(writeText).toHaveBeenCalledWith("hello");
		});

		it("readClipboard uses navigator.clipboard.readText", async () => {
			vi.spyOn(navigator.clipboard, "readText").mockResolvedValue("clipboard text");

			await expect(readClipboard()).resolves.toBe("clipboard text");
		});

		it("readClipboard propagates a denied navigator.clipboard.readText with no fallback", async () => {
			const err = new DOMException("Read permission denied.", "NotAllowedError");
			vi.spyOn(navigator.clipboard, "readText").mockRejectedValue(err);

			await expect(readClipboard()).rejects.toBe(err);
		});

		it("writeClipboard falls back to execCommand('copy') when the Clipboard API is denied", async () => {
			const err = new DOMException("Write permission denied.", "NotAllowedError");
			vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(err);
			// happy-dom doesn't implement execCommand at all.
			const execCommand = vi.fn().mockReturnValue(true);
			document.execCommand = execCommand;

			await expect(writeClipboard("fallback text")).resolves.toBeUndefined();

			expect(execCommand).toHaveBeenCalledWith("copy");
		});

		it("writeClipboard's execCommand fallback stages a textarea with the text and removes it afterward", async () => {
			vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(new DOMException("denied", "NotAllowedError"));
			let capturedValue = "";
			document.execCommand = vi.fn().mockImplementation(() => {
				const active = document.activeElement as HTMLTextAreaElement | null;
				capturedValue = active?.value ?? "";
				return true;
			});

			await writeClipboard("staged text");

			expect(capturedValue).toBe("staged text");
			expect(document.querySelector("textarea")).toBeNull();
		});

		it("writeClipboard rejects with the original error when the execCommand fallback also fails", async () => {
			const err = new DOMException("Write permission denied.", "NotAllowedError");
			vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(err);
			document.execCommand = vi.fn().mockReturnValue(false);

			await expect(writeClipboard("hello")).rejects.toBe(err);
		});

		it("writeClipboard rejects with the original error when execCommand itself throws", async () => {
			const err = new DOMException("Write permission denied.", "NotAllowedError");
			vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(err);
			document.execCommand = vi.fn().mockImplementation(() => {
				throw new Error("execCommand unsupported");
			});

			await expect(writeClipboard("hello")).rejects.toBe(err);
		});

		it("writeClipboard's execCommand fallback restores focus to whatever was focused before it stole it", async () => {
			vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(new DOMException("denied", "NotAllowedError"));
			document.execCommand = vi.fn().mockReturnValue(true);
			const input = document.createElement("input");
			document.body.appendChild(input);
			input.focus();
			expect(document.activeElement).toBe(input);

			await writeClipboard("hello");

			// A terminal (or any other input) must not silently stop receiving
			// keyboard input just because a clipboard fallback ran behind it.
			expect(document.activeElement).toBe(input);
			document.body.removeChild(input);
		});

		it("writeClipboard falls back to execCommand when navigator.clipboard doesn't exist at all (insecure context)", async () => {
			// Reported in production: Safari/WebKit on an insecure (plain HTTP,
			// non-localhost) origin doesn't expose navigator.clipboard at all — it's
			// not merely a rejected write, the property itself is undefined, throwing
			// a TypeError synchronously on `.writeText`.
			const originalClipboard = navigator.clipboard;
			Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
			let selectedRange: [number, number] | null = null;
			document.execCommand = vi.fn().mockImplementation(() => {
				const active = document.activeElement as HTMLTextAreaElement | null;
				selectedRange = active ? [active.selectionStart, active.selectionEnd] : null;
				return true;
			});

			try {
				await expect(writeClipboard("hello")).resolves.toBeUndefined();
				expect(selectedRange).toEqual([0, "hello".length]);
			} finally {
				Object.defineProperty(navigator, "clipboard", { value: originalClipboard, configurable: true });
			}
		});

		it("writeClipboard's execCommand fallback still removes the textarea and restores focus when execCommand throws", async () => {
			vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(new DOMException("denied", "NotAllowedError"));
			document.execCommand = vi.fn().mockImplementation(() => {
				throw new Error("execCommand unsupported");
			});
			const input = document.createElement("input");
			document.body.appendChild(input);
			input.focus();

			await expect(writeClipboard("hello")).rejects.toThrow();

			expect(document.querySelector("textarea")).toBeNull();
			expect(document.activeElement).toBe(input);
			document.body.removeChild(input);
		});
	});

	describe("writeClipboardAsync", () => {
		afterEach(() => setTauriMode(true));

		it("Tauri mode: awaits the promise, then routes through the native plugin (no gesture requirement)", async () => {
			setTauriMode(true);
			mockInvoke.mockResolvedValue(undefined);

			await writeClipboardAsync(Promise.resolve("resolved text"));

			expect(mockInvoke).toHaveBeenCalledWith("plugin:clipboard-manager|write_text", {
				text: "resolved text",
				label: undefined,
			});
		});

		it("browser mode: calls navigator.clipboard.write() with a ClipboardItem carrying the pending promise, never awaiting it first", async () => {
			setTauriMode(false);
			const originalClipboardItem = globalThis.ClipboardItem;
			// biome-ignore lint/suspicious/noExplicitAny: test double for a DOM constructor
			(globalThis as any).ClipboardItem = FakeClipboardItem;
			const writeSpy = vi.spyOn(navigator.clipboard, "write").mockResolvedValue(undefined);
			let resolveText!: (v: string) => void;
			const pending = new Promise<string>((resolve) => {
				resolveText = resolve;
			});

			try {
				const done = writeClipboardAsync(pending);
				// Give the synchronous call inside writeClipboardAsync a chance to run
				// before the round-trip promise ever resolves.
				await Promise.resolve();
				expect(writeSpy).toHaveBeenCalledTimes(1);
				const item = writeSpy.mock.calls[0][0][0] as unknown as FakeClipboardItem;
				expect(item.types).toEqual(["text/plain"]);

				resolveText("late text");
				const blob = await item.init["text/plain"];
				await expect(blob.text()).resolves.toBe("late text");
				await done;
			} finally {
				globalThis.ClipboardItem = originalClipboardItem;
			}
		});

		it("browser mode: falls back to writeClipboard(await textPromise) when ClipboardItem is unavailable", async () => {
			setTauriMode(false);
			const originalClipboardItem = globalThis.ClipboardItem;
			// @ts-expect-error simulating a browser without ClipboardItem support
			delete globalThis.ClipboardItem;
			const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue(undefined);

			try {
				await writeClipboardAsync(Promise.resolve("fallback text"));
				expect(writeText).toHaveBeenCalledWith("fallback text");
			} finally {
				globalThis.ClipboardItem = originalClipboardItem;
			}
		});

		it("browser mode: falls back to writeClipboard when navigator.clipboard.write rejects", async () => {
			setTauriMode(false);
			const originalClipboardItem = globalThis.ClipboardItem;
			// biome-ignore lint/suspicious/noExplicitAny: test double for a DOM constructor
			(globalThis as any).ClipboardItem = FakeClipboardItem;
			vi.spyOn(navigator.clipboard, "write").mockRejectedValue(new DOMException("denied", "NotAllowedError"));
			const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue(undefined);

			try {
				await writeClipboardAsync(Promise.resolve("rejected-write text"));
				expect(writeText).toHaveBeenCalledWith("rejected-write text");
			} finally {
				globalThis.ClipboardItem = originalClipboardItem;
			}
		});

		it("propagates a rejected textPromise as a failure", async () => {
			setTauriMode(true);
			await expect(writeClipboardAsync(Promise.reject(new Error("selection read failed")))).rejects.toThrow(
				"selection read failed",
			);
		});
	});

	describe("copyPathToClipboard", () => {
		// Shared by every "Copy Path" context-menu action (CodeEditorTab, TabBar,
		// MarkdownTab, MarkdownPanel) — none of them have a success/failure UI of
		// their own, so a rejection has nowhere to surface but the log.
		it("logs to appLogger and does not throw when the clipboard write is denied", async () => {
			setTauriMode(false);
			const err = new DOMException("Write permission denied.", "NotAllowedError");
			vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(err);
			document.execCommand = vi.fn().mockReturnValue(false);
			const errorSpy = vi.spyOn(appLogger, "error").mockImplementation(() => {});

			expect(() => copyPathToClipboard("/some/path")).not.toThrow();
			await vi.waitFor(() => {
				expect(errorSpy).toHaveBeenCalledWith("app", "Failed to copy path", err);
			});
		});

		it("does not log when the clipboard write succeeds", async () => {
			const errorSpy = vi.spyOn(appLogger, "error").mockImplementation(() => {});
			mockInvoke.mockResolvedValue(undefined);

			copyPathToClipboard("/some/path");

			await vi.waitFor(() => {
				expect(mockInvoke).toHaveBeenCalledWith("plugin:clipboard-manager|write_text", {
					text: "/some/path",
					label: undefined,
				});
			});
			expect(errorSpy).not.toHaveBeenCalled();
		});

		// In Tauri mode writeClipboard() routes through the native clipboard-manager
		// plugin (WKWebView rejects navigator.clipboard.writeText), so assert on the invoke.
		const copiedText = () =>
			mockInvoke.mock.calls.find(([cmd]) => cmd === "plugin:clipboard-manager|write_text")?.[1].text;

		it("compresses the user's home directory to ~", async () => {
			mockInvoke.mockResolvedValue(undefined);
			copyPathToClipboard("/Users/someone/Gits/project/src/main.rs");
			await vi.waitFor(() => expect(copiedText()).toBe("~/Gits/project/src/main.rs"));
		});

		it("leaves a path outside the home directory untouched", async () => {
			mockInvoke.mockResolvedValue(undefined);
			copyPathToClipboard("/etc/hosts");
			await vi.waitFor(() => expect(copiedText()).toBe("/etc/hosts"));
		});
	});
});
