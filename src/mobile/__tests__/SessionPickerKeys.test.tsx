import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import capturedSessions from "../../../docs/evidence/mobile-pickers-1972/session-state.json";
import { rpc } from "../../transport";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import type { SessionInfo } from "../useSessions";

vi.mock("../../transport", () => ({
	buildHttpUrl: (path: string) => `http://localhost${path}`,
	rpc: vi.fn().mockResolvedValue(undefined),
	HttpRpcError: class extends Error {},
}));
let receiveLine: ((text: string | null) => void) | undefined;
vi.mock("../components/OutputView", () => ({
	OutputView: (props: { onInputLine?: (text: string | null) => void }) => {
		receiveLine = props.onInputLine;
		return <div />;
	},
}));
vi.mock("../useMobileVoice", () => ({
	useMobileVoice: () => ({
		available: () => false,
		armed: () => false,
		recording: () => false,
		phase: () => undefined,
		toggle: vi.fn(),
	}),
}));

beforeEach(() => {
	vi.spyOn(console, "warn").mockImplementation(() => {});
});

afterEach(() => {
	cleanup();
	// Capture the composed screen's existing Solid owner warning, but reject
	// any other warning from this flow.
	for (const warning of vi.mocked(console.warn).mock.calls) {
		expect(warning).toEqual(["computations created outside a `createRoot` or `render` will never be disposed"]);
	}
	vi.restoreAllMocks();
	vi.clearAllMocks();
	receiveLine = undefined;
});

function mountSession(sessionExists = true) {
	return render(() => (
		<SessionDetailScreen
			subscribeChatWake={() => () => {}}
			session={capturedSessions[0] as SessionInfo}
			sessionExists={sessionExists}
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	));
}

// Real Claude AskUserQuestion: awaiting_input without choice_prompt. The atomic
// reply composer must not swallow keys belonging to the terminal's open picker.
it.each([
	["↑", "\x1b[A"],
	["↓", "\x1b[B"],
	["Tab", "\t"],
])("forwards picker %s instead of dropping it at the atomic-reply boundary", async (label, data) => {
	const view = mountSession();
	const composer = view.getByRole("textbox") as HTMLTextAreaElement;
	fireEvent.input(composer, { target: { value: "unsent local answer" } });
	fireEvent.click(view.getByRole("button", { name: label }));
	await Promise.resolve();
	expect(rpc).toHaveBeenCalledExactlyOnceWith("write_pty", {
		sessionId: capturedSessions[0].session_id,
		data,
	});
	receiveLine!("picker selection is not shell history");
	expect(composer.value).toBe("unsent local answer");
});

it("does not send picker navigation to an ended session", () => {
	const view = mountSession(false);
	fireEvent.click(view.getByRole("button", { name: "↓" }));
	expect(rpc).not.toHaveBeenCalled();
});
