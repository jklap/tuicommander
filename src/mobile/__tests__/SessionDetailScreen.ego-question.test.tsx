import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import question from "../__fixtures__/egoQuestion.json";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import type { ChoicePrompt, SessionInfo } from "../useSessions";

vi.mock("../../transport", () => ({
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

afterEach(() => {
	cleanup();
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
