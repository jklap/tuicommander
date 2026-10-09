import { afterEach, describe, expect, it, vi } from "vitest";
import "./mocks/tauri";
import { subscribePty } from "../transport";
import { setRemoteBaseUrlLookup, setRemoteTokenLookup, setSessionConnectionLookup } from "../transportRuntime";

class Socket {
	static last: Socket;
	onopen: (() => void) | null = null;
	onmessage: ((event: { data: string }) => void) | null = null;
	onclose: ((event: { code: number; reason: string }) => void) | null = null;
	onerror: (() => void) | null = null;
	close = vi.fn();
	constructor(readonly url: string) {
		Socket.last = this;
	}
	frame(frame: Record<string, unknown>) {
		this.onmessage?.({ data: JSON.stringify(frame) });
	}
}

afterEach(() => {
	vi.unstubAllGlobals();
	setRemoteBaseUrlLookup(() => undefined);
	setRemoteTokenLookup(() => undefined);
	setSessionConnectionLookup(() => undefined);
});

describe("remote PTY metadata", () => {
	// Catches: desktop remote terminals listen to local IPC and miss all daemon metadata.
	// Retargeted from main's raw `title` frame: the daemon now applies OSC titles
	// itself (osc_title.rs) and sends the resulting name as a `renamed` frame; the
	// exit-time restore is emitted before the `exit` frame, so it still lands.
	it("delivers owner WS metadata on desktop without treating rename or parsed events as activity", async () => {
		vi.stubGlobal("__TAURI_INTERNALS__", {});
		vi.stubGlobal("WebSocket", Socket);
		setSessionConnectionLookup((id) => (id === "remote-1" ? "mint" : undefined));
		setRemoteBaseUrlLookup(() => "https://mint.example");
		setRemoteTokenLookup(() => "test-token");
		const activity = vi.fn(),
			parsed = vi.fn(),
			renamed = vi.fn(),
			exit = vi.fn();
		const pending = subscribePty("remote-1", vi.fn(), exit, {
			onActivity: activity,
			onParsed: parsed,
			onRenamed: renamed,
		});
		expect(Socket.last?.url).toBe("wss://mint.example/sessions/remote-1/stream?token=test-token");
		Socket.last.onopen?.();
		const subscription = await pending;
		try {
			Socket.last.frame({ type: "renamed", session_id: "remote-1", name: "Claude Code", is_custom: false });
			Socket.last.frame({ type: "parsed", event: { type: "shell-state", state: "idle" } });
			expect(renamed.mock.calls).toEqual([["Claude Code", false]]);
			expect(parsed).toHaveBeenCalledWith({ type: "parsed", event: { type: "shell-state", state: "idle" } });
			expect(activity).not.toHaveBeenCalled();
			Socket.last.frame({ type: "activity" });
			expect(activity).toHaveBeenCalledTimes(1);
			// A cleared name (`name: null`, another client's set_session_name with
			// no name) still reaches the tab so it can drop the custom flag.
			Socket.last.frame({ type: "renamed", session_id: "remote-1", name: null, is_custom: false });
			expect(renamed.mock.calls.at(-1)).toEqual([null, false]);
			// The exit restore precedes the exit frame on the same socket.
			Socket.last.frame({ type: "renamed", session_id: "remote-1", name: "shell", is_custom: false });
			Socket.last.frame({ type: "exit" });
			expect(renamed.mock.calls.at(-1)).toEqual(["shell", false]);
			expect(exit).toHaveBeenCalledTimes(1);
		} finally {
			subscription();
		}
	});

	// Catches: a disconnected remote terminal silently falls back to the local daemon.
	it("rejects a disconnected owner without subscribing locally", async () => {
		vi.stubGlobal("__TAURI_INTERNALS__", {});
		setSessionConnectionLookup(() => "mint");
		await expect(subscribePty("remote-1", vi.fn(), vi.fn())).rejects.toThrow("Remote connection mint not connected");
	});
});
