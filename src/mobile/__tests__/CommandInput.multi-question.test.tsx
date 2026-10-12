import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";
import type { ChoicePrompt } from "../useSessions";

const writes: string[] = [];
vi.mock("../../transport", () => ({
	buildHttpUrl: (path: string) => `http://localhost${path}`,
	rpc: vi.fn((_command: string, args: { data?: string }) => {
		if (args.data !== undefined) writes.push(args.data);
		return Promise.resolve(undefined);
	}),
	HttpRpcError: class extends Error {},
}));

afterEach(() => {
	cleanup();
	writes.length = 0;
});

const form = (title: string, labels: string[]): ChoicePrompt => ({
	title,
	options: labels.map((label, index) => ({
		key: String(index + 1),
		label,
		highlighted: index === 0,
		destructive: false,
	})),
	dismiss_key: "cancel",
	selection_mode: "navigate-enter",
});

const buttonFor = (container: HTMLElement, label: string) =>
	Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes(label))!;

// Catches: the buttons stay locked after the first answer because Claude swaps
// the first question for the next one without an empty prompt in between.
it("answers every tab of a Claude multi-question form, Submit included", async () => {
	const [prompt, setPrompt] = createSignal<ChoicePrompt>(form("Which list?", ["Lista A", "Lista B"]));
	const { container } = render(() => (
		<CommandInput sessionId="disposable" agentType="claude" choicePrompt={prompt()} />
	));

	fireEvent.click(buttonFor(container, "Lista B"));
	await waitFor(() => expect(writes).toEqual(["\x1b[B", "\r"]));

	writes.length = 0;
	setPrompt(form("Where do I publish?", ["Merge in main", "Publish from branch", "No push"]));
	fireEvent.click(buttonFor(container, "No push"));
	await waitFor(() => expect(writes).toEqual(["\x1b[B", "\x1b[B", "\r"]));

	writes.length = 0;
	setPrompt(form("Ready to submit your answers?", ["Submit answers", "Cancel"]));
	fireEvent.click(buttonFor(container, "Submit answers"));
	await waitFor(() => expect(writes).toEqual(["\r"]));
});
