import { createRoot, createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionInfo } from "../useSessions";

const sounds = vi.hoisted(() => ({
	playQuestion: vi.fn(),
	playWarning: vi.fn(),
	playError: vi.fn(),
	playCompletion: vi.fn(),
}));
const { playQuestion, playWarning, playError, playCompletion } = sounds;
vi.mock("../../notifications", () => ({ notificationManager: sounds }));

import { useMobileNotifications } from "../useMobileNotifications";

function session(overrides: Partial<NonNullable<SessionInfo["state"]>> = {}): SessionInfo {
	return {
		session_id: "s1",
		cwd: null,
		worktree_path: null,
		worktree_branch: null,
		state: { awaiting_input: false, rate_limited: false, shell_state: "idle", last_activity_ms: 0, ...overrides },
	};
}

describe("useMobileNotifications", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		localStorage.clear();
		for (const fn of [playQuestion, playWarning, playError, playCompletion]) fn.mockReset();
	});
	afterEach(() => vi.useRealTimers());

	it("plays edge-triggered question, warning, and error notifications only after an initial state", () => {
		let set!: (next: SessionInfo[]) => void;
		const dispose = createRoot((d) => {
			const [sessions, setSessions] = createSignal([session()]);
			set = setSessions;
			useMobileNotifications(sessions, () => true);
			return d;
		});
		set([session({ awaiting_input: true, rate_limited: true, last_error: "failed" })]);
		expect(playQuestion).toHaveBeenCalledOnce();
		expect(playWarning).toHaveBeenCalledOnce();
		expect(playError).toHaveBeenCalledOnce();
		set([session({ awaiting_input: true, rate_limited: true, last_error: "failed" })]);
		expect(playQuestion).toHaveBeenCalledOnce();
		dispose();
	});

	it("suppresses all sounds when the shared preference is disabled", () => {
		localStorage.setItem("tuic-mobile-sounds", "false");
		let set!: (next: SessionInfo[]) => void;
		const dispose = createRoot((d) => {
			const [sessions, setSessions] = createSignal([session()]);
			set = setSessions;
			useMobileNotifications(sessions, () => true);
			return d;
		});
		set([session({ awaiting_input: true, rate_limited: true, last_error: "failed" })]);
		expect(playQuestion).not.toHaveBeenCalled();
		expect(playWarning).not.toHaveBeenCalled();
		expect(playError).not.toHaveBeenCalled();
		dispose();
	});

	it("mutes non-list transitions without replaying them when the list returns", () => {
		let set!: (next: SessionInfo[]) => void;
		let showList!: (next: boolean) => void;
		const dispose = createRoot((d) => {
			const [sessions, setSessions] = createSignal([session()]);
			const [visible, setVisible] = createSignal(false);
			set = setSessions;
			showList = setVisible;
			useMobileNotifications(sessions, visible);
			return d;
		});
		const changed = session({ awaiting_input: true, rate_limited: true, last_error: "failed" });
		set([changed]);
		for (const sound of Object.values(sounds)) expect(sound).not.toHaveBeenCalled();
		showList(true);
		set([{ ...changed }]);
		for (const sound of Object.values(sounds)) expect(sound).not.toHaveBeenCalled();
		set([session()]);
		set([changed]);
		expect(playQuestion).toHaveBeenCalledOnce();
		expect(playWarning).toHaveBeenCalledOnce();
		expect(playError).toHaveBeenCalledOnce();
		dispose();
	});

	it.each(["screen", "document", "preference"])("mutes deferred completion when %s changes before firing", (gate) => {
		vi.setSystemTime(10_000);
		let set!: (next: SessionInfo[]) => void;
		let showList!: (next: boolean) => void;
		const dispose = createRoot((d) => {
			const [sessions, setSessions] = createSignal([session({ shell_state: "busy", agent_type: "codex" })]);
			const [visible, setVisible] = createSignal(true);
			set = setSessions;
			showList = setVisible;
			useMobileNotifications(sessions, visible);
			return d;
		});
		vi.setSystemTime(20_000);
		set([session({ agent_type: "codex" })]);
		if (gate === "screen") showList(false);
		if (gate === "document") vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
		if (gate === "preference") localStorage.setItem("tuic-mobile-sounds", "false");
		vi.advanceTimersByTime(10_000);
		expect(playCompletion).not.toHaveBeenCalled();
		dispose();
		vi.restoreAllMocks();
	});

	it("does not replay transitions observed while the document is hidden or sounds disabled", () => {
		let set!: (next: SessionInfo[]) => void;
		const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
		const dispose = createRoot((d) => {
			const [sessions, setSessions] = createSignal([session()]);
			set = setSessions;
			useMobileNotifications(sessions, () => true);
			return d;
		});
		set([session({ awaiting_input: true })]);
		expect(playQuestion).not.toHaveBeenCalled();
		visibility.mockReturnValue("visible");
		set([session({ awaiting_input: true })]);
		expect(playQuestion).not.toHaveBeenCalled();
		localStorage.setItem("tuic-mobile-sounds", "false");
		set([session({ rate_limited: true })]);
		localStorage.removeItem("tuic-mobile-sounds");
		set([session({ rate_limited: true })]);
		expect(playWarning).not.toHaveBeenCalled();
		dispose();
		vi.restoreAllMocks();
	});

	it.each([undefined, "codex"])("does not queue muted %s completion for return to the list", (agent_type) => {
		vi.setSystemTime(10_000);
		let set!: (next: SessionInfo[]) => void;
		let showList!: (next: boolean) => void;
		const dispose = createRoot((d) => {
			const [sessions, setSessions] = createSignal([session({ shell_state: "busy", agent_type })]);
			const [visible, setVisible] = createSignal(false);
			set = setSessions;
			showList = setVisible;
			useMobileNotifications(sessions, visible);
			return d;
		});
		vi.setSystemTime(20_000);
		set([session({ agent_type })]);
		expect(playCompletion).not.toHaveBeenCalled();
		showList(true);
		vi.advanceTimersByTime(10_000);
		expect(playCompletion).not.toHaveBeenCalled();
		set([session({ shell_state: "busy", agent_type })]);
		vi.advanceTimersByTime(6_000);
		set([session({ agent_type })]);
		vi.advanceTimersByTime(10_000);
		expect(playCompletion).toHaveBeenCalledOnce();
		dispose();
	});

	it("defers an agent completion and cancels it when the session disappears", () => {
		vi.setSystemTime(10_000);
		let set!: (next: SessionInfo[]) => void;
		const dispose = createRoot((d) => {
			const [sessions, setSessions] = createSignal([session({ shell_state: "busy", agent_type: "codex" })]);
			set = setSessions;
			useMobileNotifications(sessions, () => true);
			return d;
		});
		vi.setSystemTime(20_000);
		set([session({ shell_state: "idle", agent_type: "codex" })]);
		set([]);
		vi.advanceTimersByTime(10_000);
		expect(playCompletion).not.toHaveBeenCalled();
		dispose();
	});

	it("cleans a deferred completion timer when the hook is disposed", () => {
		vi.setSystemTime(10_000);
		let set!: (next: SessionInfo[]) => void;
		const dispose = createRoot((d) => {
			const [sessions, setSessions] = createSignal([session({ shell_state: "busy", agent_type: "codex" })]);
			set = setSessions;
			useMobileNotifications(sessions, () => true);
			return d;
		});
		vi.setSystemTime(20_000);
		set([session({ shell_state: "idle", agent_type: "codex" })]);
		dispose();
		vi.advanceTimersByTime(10_000);
		expect(playCompletion).not.toHaveBeenCalled();
	});

	it("replaces an earlier deferred completion when the same agent becomes busy again", () => {
		vi.setSystemTime(10_000);
		let set!: (next: SessionInfo[]) => void;
		const dispose = createRoot((d) => {
			const [sessions, setSessions] = createSignal([session({ shell_state: "busy", agent_type: "codex" })]);
			set = setSessions;
			useMobileNotifications(sessions, () => true);
			return d;
		});
		vi.setSystemTime(20_000);
		set([session({ shell_state: "idle", agent_type: "codex" })]);
		vi.advanceTimersByTime(5_000);
		set([session({ shell_state: "busy", agent_type: "codex" })]);
		vi.setSystemTime(30_000);
		set([session({ shell_state: "idle", agent_type: "codex" })]);
		vi.advanceTimersByTime(5_000);
		expect(playCompletion).not.toHaveBeenCalled();
		vi.advanceTimersByTime(5_000);
		expect(playCompletion).toHaveBeenCalledOnce();
		dispose();
	});
});
