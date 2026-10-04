import { describe, expect, it } from "vitest";
import { projectChatProtocolText } from "../../components/AIChatPanel/protocolText";

describe("AI Chat protocol text", () => {
	// Catches: a same-line reply swallowed into the status label.
	it("preserves reply text after the intent title", () => {
		expect(projectChatProtocolText("intent: Chatting (Chat)Hello there")).toEqual({
			intent: { text: "Chatting", title: "Chat" },
			body: "Hello there",
			suggestions: [],
		});
		expect(projectChatProtocolText("intent: Chatting (Chat) Hello there\nNext line").body).toBe(
			"Hello there\nNext line",
		);
	});

	// Catches: untitled intent accidentally promoted to reply text.
	it("retains untitled intent behavior", () => {
		expect(projectChatProtocolText("intent: Chatting without a title")).toEqual({
			intent: { text: "Chatting without a title", title: null },
			body: "",
			suggestions: [],
		});
	});

	// Catches: recovery parsing changing an existing titled intent with parentheses in its description.
	it("preserves parenthesized descriptions when the title ends the line", () => {
		expect(projectChatProtocolText("intent: Check retries (slow path) (Status)").intent).toEqual({
			text: "Check retries (slow path)",
			title: "Status",
		});
	});

	// Catches: recovery parsing stripping literal syntax in code examples.
	it("leaves fenced and indented intent examples unchanged", () => {
		const text = "```text\nintent: Chatting (Chat)Hello there\n```\n    intent: Chatting (Chat)Hello there";
		expect(projectChatProtocolText(text)).toEqual({ intent: null, body: text, suggestions: [] });
	});
});
