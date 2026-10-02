import { render } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { AnswersPanel } from "../AnswersPanel";

describe("AnswersPanel", () => {
	it("lists the prompt then each answer as selectable text", () => {
		const { container } = render(() => (
			<AnswersPanel view={{ prompt: "❯ q", answers: ["💬 one", "💬 two"] }} fontFamily="monospace" fontSize={13} />
		));
		const panel = container.querySelector("[data-answers-only]") as HTMLElement;
		expect(panel.textContent).toBe("❯ q💬 one💬 two");
		expect(Array.from(container.querySelectorAll("[data-answer]")).map((n) => n.textContent)).toEqual([
			"💬 one",
			"💬 two",
		]);
		// catches: the layer blocking selection/copy, or being unable to scroll a long turn
		expect(panel.style.userSelect).toBe("text");
		expect(panel.style.overflow).toBe("auto");
	});

	it("says so when the turn has no answers", () => {
		// catches: an empty, unexplained panel that looks like a hung terminal
		const { container } = render(() => (
			<AnswersPanel view={{ prompt: null, answers: [] }} fontFamily="monospace" fontSize={13} />
		));
		expect(container.textContent).toContain("No 💬 answers");
	});
});
