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

// Catches: an unchanged state snapshot unlocks an in-flight answer, letting a second
// tap interleave arrow keys and Enter and choose the wrong answer.
it("keeps the current question locked when an unchanged snapshot arrives", async () => {
	const [prompt, setPrompt] = createSignal<ChoicePrompt>(form("Which list?", ["Lista A", "Lista B"]));
	const { container } = render(() => (
		<CommandInput sessionId="disposable" agentType="claude" choicePrompt={prompt()} />
	));

	fireEvent.click(buttonFor(container, "Lista B"));
	// A polled/WS snapshot reaches the phone before the write acknowledgement.
	setPrompt(form("Which list?", ["Lista A", "Lista B"]));
	fireEvent.click(buttonFor(container, "Lista B"));

	await waitFor(() => expect(writes).toContain("\r"));
	expect(writes).toEqual(["\x1b[B", "\r"]);
});

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

// Catches: opening Grok's recorded z text option also sends Enter, submitting
// the newly opened composer before the user can type their custom answer.
it("opens the Grok custom-answer composer without submitting an empty answer", async () => {
	// Payload observed in grok-native-permission-2027.tcap (record 2).
	const prompt: ChoicePrompt = {
		title: "May I run ls?",
		options: [
			{ key: "1", label: "Yes", highlighted: true, destructive: false },
			{ key: "2", label: "No", highlighted: false, destructive: true },
			{ key: "z", label: "Type your answer here", highlighted: false, destructive: false },
		],
	};
	const { container } = render(() => <CommandInput sessionId="disposable" agentType="grok" choicePrompt={prompt} />);
	await fireEvent.click(buttonFor(container, "Type your answer here"));
	// Drain the resolved PTY writes, including any unwanted follow-up Enter.
	await new Promise((resolve) => setTimeout(resolve, 0));
	expect(writes).toEqual(["z"]);
});

// Catches: sending "10" to ego toggles option 1 instead of option 10,
// because its raw selector binds single digits only.
it("navigates and toggles ego options beyond the single-digit shortcuts", async () => {
	const prompt: ChoicePrompt = {
		...form(
			"Which checks?",
			Array.from({ length: 10 }, (_, i) => `[ ] Check ${i + 1}`),
		),
		selection_mode: "toggle-enter",
	};
	prompt.options = prompt.options.map((option, index) => ({ ...option, highlighted: index === 8 }));
	const { container } = render(() => <CommandInput sessionId="disposable" agentType="ego" choicePrompt={prompt} />);
	fireEvent.click(buttonFor(container, "Check 10"));
	await waitFor(() => expect(writes).toEqual(["\x1b[B", " "]));
	expect(writes).not.toContain("\r");
});
