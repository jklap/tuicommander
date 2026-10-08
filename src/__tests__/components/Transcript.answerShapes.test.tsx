// @vitest-environment jsdom
import { cleanup, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";

afterEach(cleanup);

async function show(text: string) {
	const view = render(() => (
		<Transcript
			entries={() => [{ id: "a", kind: "agent", text }]}
			busy={() => false}
			emptyMessage="Empty"
			observeToolDuration={false}
		/>
	));
	await Promise.resolve();
	const blocks = Array.from(view.container.querySelectorAll<HTMLElement>("#markdown-content > div > *"));
	return { ...view, blocks, marks: blocks.map((block) => block.getAttribute("data-tuic-answer")) };
}

describe("chat answer shapes follow the CLI grid extent", () => {
	// Catches: a single-line answer left raw or untinted.
	it("tints and strips a one-line answer", async () => {
		const { blocks, marks } = await show("💬 Yes.");
		expect(marks).toEqual(["end"]);
		expect(blocks[0].textContent).toBe("Yes.");
	});

	// Catches: a table, nested list or code fence ending the answer early (tint stops, tail untinted).
	it("keeps table, nested list and a fenced marker example inside the answer", async () => {
		const text =
			"💬 Result:\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- x\n  - nested\n\n```text\n💬 literal\n```\n\nClosing words.";
		const { blocks, marks, container } = await show(text);
		expect(blocks.map((b) => b.tagName)).toEqual(["P", "TABLE", "UL", "PRE", "P"]);
		expect(marks).toEqual(["", "", "", "", "end"]);
		expect(container.querySelectorAll("[data-tuic-answer-start]")).toHaveLength(1);
		expect(container.querySelector("pre")?.textContent).toContain("💬 literal");
	});

	// Catches: prose before the first marker being swept into the answer.
	it("does not tint text before the first marker", async () => {
		const { marks } = await show("Intro.\n\n💬 Answer.\n\nMore.");
		expect(marks).toEqual([null, "", "end"]);
	});

	// Catches: streaming a second answer onto a finished one leaving a raw 💬 or a stale "end" mark.
	it("re-marks each answer when a second one streams in", async () => {
		const [entries, setEntries] = createSignal<AcpTranscriptEntry[]>([
			{ id: "a", kind: "agent", text: "💬 One.\n\nMore." },
		]);
		const { container } = render(() => <Transcript entries={entries} busy={() => false} emptyMessage="Empty" />);
		await Promise.resolve();
		setEntries([{ id: "a", kind: "agent", text: "💬 One.\n\nMore.\n\n💬 Two.\n\nEnd." }]);
		await Promise.resolve();
		await Promise.resolve();
		const blocks = Array.from(container.querySelectorAll<HTMLElement>("#markdown-content > div > *"));
		expect(blocks.map((b) => b.getAttribute("data-tuic-answer"))).toEqual(["", "end", "", "end"]);
		expect(container.textContent).not.toContain("💬");
	});

	// Catches: a 💬 on its own line inside one paragraph (soft break) staying raw, though the grid row starts an answer.
	it("strips a 💬 that starts a line inside a paragraph", async () => {
		const { container } = await show("Intro line.\n💬 The answer.");
		expect(container.textContent).not.toContain("💬");
	});
});
