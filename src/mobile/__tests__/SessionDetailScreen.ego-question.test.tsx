import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import question from "../__fixtures__/egoQuestion.json";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import type { ChoicePrompt, SessionInfo } from "../useSessions";

vi.mock("../../transport", () => ({
	subscribeEvents: vi.fn(async () => () => {}),
	rpc: vi.fn().mockResolvedValue(undefined),
	buildHttpUrl: (path: string) => `http://localhost${path}`,
	HttpRpcError: class extends Error {},
}));
vi.mock("../components/OutputView", () => ({ OutputView: () => <div>Terminal output</div> }));
vi.mock("../useMobileVoice", () => ({
	useMobileVoice: () => ({
		phase: () => null,
		available: () => false,
		armed: () => false,
		recording: () => false,
	}),
}));

import { rpc } from "../../transport";

// Solid's development build warns when this isolated screen's lazy updates
// run after the event owner. Capture and validate this known fixture warning;
// unexpected warnings still fail rather than leaking raw stderr.
let warnings: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
	warnings = vi.spyOn(console, "warn").mockImplementation(() => {});
});
afterEach(() => {
	cleanup();
	for (const args of warnings.mock.calls) {
		expect(args).toEqual(["computations created outside a `createRoot` or `render` will never be disposed"]);
	}
	warnings.mockRestore();
	vi.clearAllMocks();
});

const session: SessionInfo = {
	session_id: "ego-question",
	cwd: "/repo",
	worktree_path: null,
	worktree_branch: null,
	state: {
		agent_type: "ego",
		shell_state: "idle",
		awaiting_input: true,
		question_confident: true,
		question_text: question.title,
		rate_limited: false,
		last_activity_ms: 0,
		choice_prompt: question as ChoicePrompt,
	},
};

// Catches: waiting ego questions showing no readable, explicit mobile question action.
it("opens the full recorded ego question on demand and submits the selected option once", async () => {
	const view = render(() => (
		<SessionDetailScreen
			subscribeChatWake={() => () => {}}
			session={session}
			sessionExists
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	));
	expect(view.container.textContent).toContain("Input");
	expect(view.queryByText(question.title)).toBeNull();
	fireEvent.click(view.getByRole("button", { name: "Open ego question" }));
	expect(view.getByText(question.title)).toBeTruthy();
	for (const option of question.options) expect(view.getByText(option.label)).toBeTruthy();
	const answer = view.getByRole("button", { name: new RegExp(question.options[1].label) });
	fireEvent.click(answer);
	fireEvent.click(answer);
	await waitFor(() =>
		expect(vi.mocked(rpc).mock.calls.filter(([cmd, args]) => cmd === "write_pty" && args?.data === "\r")).toHaveLength(
			1,
		),
	);
	expect(
		vi
			.mocked(rpc)
			.mock.calls.filter(([cmd]) => cmd === "write_pty")
			.map(([, args]) => args?.data),
	).toEqual(["\x1b[B", "\r"]);
});

// Catches: native free-text waits have no choice_prompt, so the ego question
// opener used to disappear despite the authoritative awaiting state.
it("opens a recorded free-text ego question without requiring numbered options", () => {
	const view = render(() => (
		<SessionDetailScreen
			subscribeChatWake={() => () => {}}
			session={{
				...session,
				state: { ...session.state!, question_text: "Which release checks should we run?", choice_prompt: undefined },
			}}
			sessionExists
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	));
	expect(view.container.textContent).toContain("Input");
	fireEvent.click(view.getByRole("button", { name: "Open ego question" }));
	expect(view.getByText("Which release checks should we run?")).toBeTruthy();
	expect(view.getByRole("textbox")).toBeTruthy();
	expect(view.getByRole("button", { name: "Send" })).toBeTruthy();
});

// Catches: ego multiple selectors getting no answer card, or a checkbox tap
// submitting before the user has finished selecting options.
it("toggles recorded ego multiple options and submits only on explicit request", async () => {
	const multiple: ChoicePrompt = {
		title: "Which release checks should we run?",
		options: [
			{
				key: "1",
				label: "[ ] Parser regression Replay the recorded terminal output",
				highlighted: true,
				destructive: false,
			},
			{
				key: "2",
				label: "[ ] Mobile answer controls Verify the mobile question card",
				highlighted: false,
				destructive: false,
			},
		],
		selection_mode: "toggle-enter",
		dismiss_key: "cancel",
	};
	const [currentSession, setCurrentSession] = createSignal<SessionInfo>({
		...session,
		state: { ...session.state!, question_text: multiple.title, choice_prompt: multiple },
	});
	const acknowledgeToggle = (index: number) =>
		setCurrentSession((previous) => ({
			...previous,
			state: {
				...previous.state!,
				choice_prompt: {
					...multiple,
					options: multiple.options.map((option, i) => ({
						...option,
						highlighted: i === index,
						label: i <= index ? option.label.replace("[ ]", "[x]") : option.label,
					})),
				},
			},
		}));
	const view = render(() => (
		<SessionDetailScreen
			subscribeChatWake={() => () => {}}
			session={currentSession()}
			sessionExists
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	));
	fireEvent.click(view.getByRole("button", { name: "Open ego question" }));
	fireEvent.click(view.getByRole("button", { name: /Parser regression/ }));
	await waitFor(() =>
		expect(
			vi
				.mocked(rpc)
				.mock.calls.filter(([cmd]) => cmd === "write_pty")
				.map(([, args]) => args?.data),
		).toEqual(["1"]),
	);
	expect((view.getByRole("button", { name: /Mobile answer controls/ }) as HTMLButtonElement).disabled).toBe(true);
	acknowledgeToggle(0);
	await waitFor(() =>
		expect((view.getByRole("button", { name: /Mobile answer controls/ }) as HTMLButtonElement).disabled).toBe(false),
	);
	fireEvent.click(view.getByRole("button", { name: /Mobile answer controls/ }));
	await waitFor(() =>
		expect(
			vi
				.mocked(rpc)
				.mock.calls.filter(([cmd]) => cmd === "write_pty")
				.map(([, args]) => args?.data),
		).toEqual(["1", "2"]),
	);
	acknowledgeToggle(1);
	await waitFor(() =>
		expect((view.getByRole("button", { name: "Submit answers" }) as HTMLButtonElement).disabled).toBe(false),
	);
	fireEvent.click(view.getByRole("button", { name: "Submit answers" }));
	fireEvent.click(view.getByRole("button", { name: "Submit answers" }));
	await waitFor(() =>
		expect(
			vi
				.mocked(rpc)
				.mock.calls.filter(([cmd]) => cmd === "write_pty")
				.map(([, args]) => args?.data),
		).toEqual(["1", "2", "\r"]),
	);
});
