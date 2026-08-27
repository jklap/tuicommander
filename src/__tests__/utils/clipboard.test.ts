import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { appLogger } from "../../stores/appLogger";
import { copyPathToClipboard, readClipboard, writeClipboard } from "../../utils/clipboard";
import { mockInvoke } from "../mocks/tauri";

describe("copyPathToClipboard", () => {
	beforeEach(() => {
		mockInvoke.mockClear();
	});

	// In Tauri mode writeClipboard() routes through the native clipboard
	// command (WKWebView rejects navigator.clipboard.writeText), so assert on the invoke.
	const copiedText = () => mockInvoke.mock.calls.find(([cmd]) => cmd === "write_clipboard_text")?.[1].text;

	it("compresses the user's home directory to ~", async () => {
		copyPathToClipboard("/Users/someone/Gits/project/src/main.rs");
		await vi.waitFor(() => expect(copiedText()).toBe("~/Gits/project/src/main.rs"));
	});

	it("leaves a path outside the home directory untouched", async () => {
		copyPathToClipboard("/etc/hosts");
		await vi.waitFor(() => expect(copiedText()).toBe("/etc/hosts"));
	});
});

describe("native clipboard commands", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
	});

	it("writes through the native command so removing the plugin cannot break copy", async () => {
		await writeClipboard("hello\nworld");
		expect(mockInvoke).toHaveBeenCalledExactlyOnceWith("write_clipboard_text", { text: "hello\nworld" });
	});

	it("reads through the native command so paste does not use the removed plugin", async () => {
		mockInvoke.mockResolvedValueOnce("pasted text");
		await expect(readClipboard()).resolves.toBe("pasted text");
		expect(mockInvoke).toHaveBeenCalledExactlyOnceWith("read_clipboard_text");
	});

	it("rejects native write failures so callers can show copy failed", async () => {
		mockInvoke.mockRejectedValueOnce("clipboard unavailable");
		await expect(writeClipboard("text")).rejects.toBe("clipboard unavailable");
	});

	it("rejects native read failures instead of returning empty text", async () => {
		mockInvoke.mockRejectedValueOnce("clipboard unavailable");
		await expect(readClipboard()).rejects.toBe("clipboard unavailable");
	});
});

describe("browser clipboard isolation", () => {
	beforeEach(() => {
		vi.stubGlobal("__TAURI_SHIM__", true);
		mockInvoke.mockReset();
	});

	afterEach(() => vi.unstubAllGlobals());

	it("writes only to the browser clipboard so remote copy cannot change the host pasteboard", async () => {
		const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValueOnce();
		await writeClipboard("client text");
		expect(writeText).toHaveBeenCalledWith("client text");
		expect(mockInvoke).not.toHaveBeenCalled();
	});

	it("reads only the browser clipboard so remote paste cannot disclose the host pasteboard", async () => {
		vi.spyOn(navigator.clipboard, "readText").mockResolvedValueOnce("client text");
		await expect(readClipboard()).resolves.toBe("client text");
		expect(mockInvoke).not.toHaveBeenCalled();
	});
});

describe("browser-mode clipboard failures", () => {
	beforeEach(() => {
		vi.stubGlobal("__TAURI_SHIM__", true);
		mockInvoke.mockReset();
	});

	afterEach(() => {
		vi.unstubAllGlobals();
		vi.restoreAllMocks();
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

describe("copyPathToClipboard failure logging", () => {
	afterEach(() => {
		vi.unstubAllGlobals();
		vi.restoreAllMocks();
	});

	// Shared by every "Copy Path" context-menu action (CodeEditorTab, TabBar,
	// MarkdownTab, MarkdownPanel) — none of them have a success/failure UI of
	// their own, so a rejection has nowhere to surface but the log.
	it("logs to appLogger and does not throw when the clipboard write is denied", async () => {
		vi.stubGlobal("__TAURI_SHIM__", true);
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
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
		const errorSpy = vi.spyOn(appLogger, "error").mockImplementation(() => {});

		copyPathToClipboard("/some/path");

		await vi.waitFor(() => {
			expect(mockInvoke).toHaveBeenCalledWith("write_clipboard_text", { text: "/some/path" });
		});
		expect(errorSpy).not.toHaveBeenCalled();
	});
});
