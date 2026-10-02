import { render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AnswersPanel } from "../AnswersPanel";

const panelOf = (container: HTMLElement) => container.querySelector("[data-answers-only]") as HTMLElement;

describe("AnswersPanel", () => {
	afterEach(() => vi.restoreAllMocks());

	it("lists each prompt followed by its answers as selectable text", () => {
		const { container } = render(() => (
			<AnswersPanel
				view={[
					{ prompt: "❯ q1", answers: ["💬 one", "💬 two"] },
					{ prompt: "❯ q2", answers: ["💬 three"] },
				]}
				fontFamily="monospace"
				fontSize={13}
			/>
		));
		const panel = panelOf(container);
		expect(panel.textContent).toBe("❯ q1💬 one💬 two❯ q2💬 three");
		expect(Array.from(container.querySelectorAll("[data-prompt]")).map((n) => n.textContent)).toEqual(["❯ q1", "❯ q2"]);
		// catches: the layer blocking selection/copy, or being unable to scroll a long history
		expect(panel.style.userSelect).toBe("text");
		expect(panel.style.overflow).toBe("auto");
	});

	it("renders no row for a turn without a 💬 answer", () => {
		// catches: a placeholder or bare prompt row printed for an answerless turn
		const { container } = render(() => (
			<AnswersPanel
				view={[
					{ prompt: "❯ old", answers: [] },
					{ prompt: "❯ answered", answers: ["💬 yes"] },
					{ prompt: "❯ asked just now", answers: [] },
				]}
				fontFamily="monospace"
				fontSize={13}
			/>
		));
		expect(panelOf(container).textContent).toBe("❯ answered💬 yes");
		expect(container.querySelectorAll("[data-turn]")).toHaveLength(1);
	});

	it("renders a 300-character multi-line prompt in full", () => {
		// catches: the prompt truncated to one line
		const prompt = `❯ ${"a".repeat(100)}\n  ${"b".repeat(100)}\n  ${"c".repeat(96)}`;
		const { container } = render(() => (
			<AnswersPanel view={[{ prompt, answers: ["💬 a"] }]} fontFamily="monospace" fontSize={13} />
		));
		expect(container.querySelector("[data-prompt]")?.textContent).toBe(prompt);
	});

	it("opens scrolled to the newest entry", () => {
		// catches: the panel opening at the oldest question of a long history
		vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockReturnValue(1000);
		const { container } = render(() => (
			<AnswersPanel view={[{ prompt: "❯ q", answers: ["💬 a"] }]} fontFamily="monospace" fontSize={13} />
		));
		expect(panelOf(container).scrollTop).toBe(1000);
	});
});
