// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import { acpTranscript } from "../../stores/acpTranscript";
import { transcriptPreferences } from "../../stores/transcriptPreferences";

function append(text: string) {
	acpTranscript.applyFrame({
		kind: "event",
		connectionId: "test",
		generation: 1,
		sequence: 1,
		sessionId: "chat",
		turnId: "turn",
		event: { kind: "sessionUpdate", update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text } } },
	});
}

async function setup(mobile = false) {
	append("Read this earlier answer.");
	const [busy, setBusy] = createSignal(false);
	const view = render(() => (
		<Transcript mobile={mobile} entries={() => acpTranscript.entries("chat")} busy={busy} emptyMessage="Empty" />
	));
	await Promise.resolve();
	const transcript = view.getByLabelText("Chat transcript");
	// jsdom has no layout. These scroll metrics model the browser geometry;
	// the worktree Vite check separately verifies real overflow and positioning.
	Object.defineProperties(transcript, {
		clientHeight: { configurable: true, value: 200 },
		scrollHeight: { configurable: true, value: 900 },
	});
	const scroll = (top: number) => {
		transcript.scrollTop = top;
		fireEvent.scroll(transcript);
	};
	return { ...view, transcript, scroll, setBusy };
}

beforeEach(() => {
	acpTranscript.reset();
	transcriptPreferences.setEnabled(true);
});
afterEach(() => {
	cleanup();
	window.getSelection()?.removeAllRanges();
});

describe("Transcript new text", () => {
	// Catches: streaming into an existing message being missed, or clicking the pill leaving follow disabled.
	it.each([false, true])(
		"shows detached streaming text and resumes follow after clicking (mobile=%s)",
		async (mobile) => {
			const view = await setup(mobile);
			view.scroll(300);
			append(" More text in the same message.");
			await Promise.resolve();
			expect(view.transcript.scrollTop).toBe(300);
			fireEvent.click(view.getByRole("button", { name: "New text below" }));
			expect(view.transcript.scrollTop).toBe(900);
			expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
			view.transcript.scrollTop = 700;
			append(" Following again.");
			await Promise.resolve();
			expect(view.transcript.scrollTop).toBe(900);
			expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
		},
	);

	// Catches: a pending arrival staying visible after the reader manually reaches the bottom.
	it("clears a new-entry indicator when scrolling to the bottom by hand", async () => {
		const view = await setup();
		view.scroll(300);
		acpTranscript.noteUserMessage("chat", "A new prompt");
		await Promise.resolve();
		expect(view.getByRole("button", { name: "New text below" })).toBeTruthy();
		view.scroll(700);
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
	});

	// Catches: bottom-follow output or a typing-state change being counted as unread text.
	it("does not announce text already followed or a busy-only rerender while detached", async () => {
		const view = await setup();
		view.scroll(700);
		append(" Visible at the bottom.");
		await Promise.resolve();
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
		view.scroll(300);
		view.setBusy(true);
		await Promise.resolve();
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
		expect(view.transcript.scrollTop).toBe(300);
	});

	// Catches: search detachment itself showing a pill, or search disabling arrival detection permanently.
	it("leaves a search match alone until subsequent content arrives", async () => {
		const view = await setup();
		fireEvent.keyDown(view.transcript, { key: "f", ctrlKey: true });
		await Promise.resolve();
		const input = view.getByRole("textbox", { name: "Find in chat" });
		fireEvent.input(input, { target: { value: "earlier" } });
		fireEvent.keyDown(input, { key: "Enter" });
		expect(window.getSelection()?.toString()).toBe("earlier");
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
		append(" New arrival after search.");
		await Promise.resolve();
		expect(view.getByRole("button", { name: "New text below" })).toBeTruthy();
	});

	// Catches: the preference hiding only a stale DOM element while future arrivals recreate it.
	it("clears pending text when disabled and keeps new arrivals hidden", async () => {
		const view = await setup();
		view.scroll(300);
		append(" First unread text.");
		await Promise.resolve();
		expect(view.getByRole("button", { name: "New text below" })).toBeTruthy();
		transcriptPreferences.setEnabled(false);
		append(" Further unread text.");
		await Promise.resolve();
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
		expect(view.transcript.scrollTop).toBe(300);
		transcriptPreferences.setEnabled(true);
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
	});

	// Catches: a cleared/switched conversation inheriting another conversation's pending pill.
	it("drops pending arrivals when the conversation is replaced", async () => {
		const view = await setup();
		view.scroll(300);
		append(" Unread text.");
		await Promise.resolve();
		expect(view.getByRole("button", { name: "New text below" })).toBeTruthy();
		acpTranscript.clear("chat");
		acpTranscript.noteUserMessage("chat", "Different conversation");
		await Promise.resolve();
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
	});
});
