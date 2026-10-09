// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";

afterEach(() => {
	cleanup();
	window.getSelection()?.removeAllRanges();
});

async function search(entries: AcpTranscriptEntry[], query: string) {
	const view = render(() => <Transcript entries={() => entries} busy={() => false} emptyMessage="Empty" />);
	await Promise.resolve();
	fireEvent.keyDown(view.getByLabelText("Chat transcript"), { key: "f", ctrlKey: true });
	await Promise.resolve();
	fireEvent.input(view.getByRole("textbox", { name: "Find in chat" }), { target: { value: query } });
	fireEvent.keyDown(view.getByRole("textbox", { name: "Find in chat" }), { key: "Enter" });
	return view;
}

describe("transcript search critic regressions", () => {
	it("finds a visible phrase split by Markdown emphasis instead of falsely reporting no match", async () => {
		await search([{ id: "answer", kind: "agent", text: "Use **git rebase** carefully." }], "Use git rebase");
		expect(window.getSelection()?.toString()).toBe("Use git rebase");
	});

	it("does not consume the first navigation result with text in a collapsed thinking section", async () => {
		const view = await search(
			[
				{ id: "reasoning", kind: "thought", text: "deployment hidden reasoning" },
				{ id: "answer", kind: "user", text: "deployment visible answer" },
			],
			"deployment",
		);
		const selection = window.getSelection();
		expect(selection?.toString()).toBe("deployment");
		const selectedElement = selection?.getRangeAt(0).startContainer.parentElement;
		const disclosure = selectedElement?.closest("details");
		// A hit must be visible: skip a closed disclosure or open it before selecting its text.
		expect(disclosure === null || disclosure?.open).toBe(true);
		expect(view.getByText("deployment visible answer")).toBeTruthy();
	});
});
