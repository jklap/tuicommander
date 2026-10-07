// @vitest-environment jsdom
import { cleanup, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";

afterEach(cleanup);

function transcript(entries: AcpTranscriptEntry[]) {
	return render(() => (
		<Transcript entries={() => entries} busy={() => false} emptyMessage="Empty" observeToolDuration={false} />
	));
}

describe("transcript presentation", () => {
	// Catches: harness-authored interruptions/inbox wakes impersonating a human prompt.
	it("renders harness notices separately from user bubbles and leaves quoted notices alone", () => {
		const { getAllByRole, container } = transcript([
			{ id: "wake", kind: "user", text: "[TUIC] message available — read it with: agent action=inbox" },
			{ id: "interrupt", kind: "user", text: "[Request interrupted by user for tool use]" },
			{ id: "human", kind: "user", text: "Explain [Request interrupted by user for tool use]" },
		]);
		expect(getAllByRole("note").map((note) => note.textContent)).toEqual([
			"[TUIC] message available — read it with: agent action=inbox",
			"[Request interrupted by user for tool use]",
		]);
		expect(container.querySelectorAll('button[aria-label="Copy user message"]')).toHaveLength(1);
	});

	// Catches: loading a completed tool history inventing a 0.0s execution duration.
	it("shows historical tool status without a duration measured since mounting", () => {
		const { container } = transcript([
			{ id: "tool", kind: "tool", call: { toolCallId: "tool", title: "Read", status: "completed" } },
		]);
		expect(container.querySelector("summary")?.textContent).toContain("Completed");
		expect(container.querySelector("summary")?.textContent).not.toMatch(/\d+\.\d+s observed/);
	});

	// Catches: empty and successive thinking blocks reserving multiple disclosure rows.
	it("omits empty thoughts and merges adjacent thoughts without crossing user turns", () => {
		const { container } = transcript([
			{ id: "empty", kind: "thought", text: " \n" },
			{ id: "t1", kind: "thought", text: "First thought" },
			{ id: "hidden", kind: "agent", text: " " },
			{ id: "t2", kind: "thought", text: "Second thought" },
			{ id: "u", kind: "user", text: "Next turn" },
			{ id: "t3", kind: "thought", text: "New turn thought" },
		]);
		expect(container.querySelectorAll("details")).toHaveLength(2);
		expect(container.querySelector("details")?.textContent).toContain("First thought\n\nSecond thought");
	});

	// Catches: hidden protocol-only/whitespace assistant entries leaving blank gaps and orphan Copy buttons.
	it("allocates no message row or copy action for invisible assistant entries", () => {
		const { container } = transcript([
			{ id: "a", kind: "agent", text: "Visible paragraph" },
			{ id: "empty", kind: "agent", text: " \n" },
			{ id: "suggest", kind: "agent", text: "suggest: [ One | Two | Three ]" },
			{ id: "ack", kind: "agent", text: "TUICommander v1.8.0 is connected." },
			{ id: "u", kind: "user", text: "Actual prompt" },
		]);
		expect(container.querySelectorAll('button[aria-label="Copy assistant message"]')).toHaveLength(1);
		expect(container.querySelector('[aria-label="Chat transcript"]')?.children).toHaveLength(2);
	});

	// Catches: Claude image markers being printed verbatim rather than presented as attachments.
	it("renders numbered and unnumbered image markers as chips while preserving other text", () => {
		const { container, getAllByRole } = transcript([
			{ id: "u", kind: "user", text: "Review this [Image #1]\n[image]\n[image] and keep [image processing]" },
		]);
		expect(getAllByRole("img").map((chip) => chip.getAttribute("aria-label"))).toEqual([
			"Image attachment 1",
			"Image attachment",
		]);
		expect(container.textContent).toContain("Review this");
		expect(container.textContent).toContain("[image processing]");
		expect(container.textContent).not.toContain("[image]");
		expect(container.textContent).not.toContain("[Image #1]");
	});

	// Catches: TUIC answer markers appearing as raw emoji with no CLI-equivalent highlighting.
	it("highlights marked answer paragraphs and preserves marker examples in code and ordinary prose", async () => {
		const [entries, setEntries] = createSignal<AcpTranscriptEntry[]>([
			{
				id: "a",
				kind: "agent",
				text: "💬 **Answer** with a link https://example.com.\n\n```text\n💬 example\n```\n\nMention 💬 inside prose.",
			},
		]);
		const { container } = render(() => <Transcript entries={entries} busy={() => false} emptyMessage="Empty" />);
		await Promise.resolve();
		expect(container.querySelector("[data-tuic-answer]")?.textContent).toBe("Answer with a link https://example.com.");
		expect(container.querySelector("pre")?.textContent).toContain("💬 example");
		expect(container.textContent).toContain("Mention 💬 inside prose.");
		setEntries([{ id: "a", kind: "agent", text: "💬 **Updated answer**" }]);
		await Promise.resolve();
		expect(container.querySelector("[data-tuic-answer]")?.textContent).toBe("Updated answer");
	});
});
