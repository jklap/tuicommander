import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { Composer } from "../../components/AIChatPanel/Composer";
import { aiChatDraft } from "../../components/AIChatPanel/draft";
import type { AcpChat } from "../../components/AIChatPanel/useAcpChat";

afterEach(() => {
	cleanup();
	aiChatDraft.reset();
	vi.restoreAllMocks();
});

it("does not attach a startup-delayed paste to a different chat tab", async () => {
	aiChatDraft.reset();
	const [sessionId, setSessionId] = createSignal("original-chat");
	let connected = false;
	let finishStart!: () => void;
	const starting = new Promise<void>((resolve) => {
		finishStart = resolve;
	});
	const chat = {
		sessionId,
		busy: () => false,
		queuedPrompts: () => [],
		capabilities: () => (connected ? { promptImage: true } : null),
		ensureStarted: () => starting,
	} as unknown as AcpChat;
	vi.spyOn(FileReader.prototype, "readAsDataURL").mockImplementation(function (this: FileReader) {
		Object.defineProperty(this, "result", { value: "data:image/png;base64,cG5n" });
		this.onload?.(new ProgressEvent("load") as ProgressEvent<FileReader>);
	});
	const { container } = render(() => <Composer chat={chat} />);
	const file = new File(["png"], "screenshot.png", { type: "image/png" });
	fireEvent.paste(container.querySelector("textarea")!, {
		clipboardData: {
			items: [{ type: "image/png", getAsFile: () => file }],
			getData: () => "",
		},
	});
	setSessionId("other-chat");
	connected = true;
	finishStart();
	await starting;
	await Promise.resolve();
	await Promise.resolve();
	expect(container.querySelector('img[alt="Pasted image"]')).toBeNull();
	expect(aiChatDraft.images()).toEqual([]);
});
