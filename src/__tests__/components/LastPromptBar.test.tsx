import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { LastPromptBar } from "../../components/Terminal/LastPromptBar";

afterEach(cleanup);

describe("LastPromptBar", () => {
	it("shows the PTY description and last prompt together", async () => {
		const { container } = render(() => (
			<LastPromptBar ptyDescription={() => "Validating the release"} prompt={() => "Run the full release checks"} />
		));

		expect(container.textContent).toContain("Validating the release");
		expect(container.textContent).toContain("Prompt: Run the full release checks");

		await fireEvent.click(container.firstElementChild!);
		expect(container.textContent).toContain("PTY");
		expect(container.textContent).toContain("Prompt");
	});

	it("falls back to the last prompt when no PTY description exists", () => {
		const { container } = render(() => (
			<LastPromptBar ptyDescription={() => null} prompt={() => "Fix the failing test"} />
		));

		expect(container.textContent).toContain("Prompt: Fix the failing test");
	});
});
