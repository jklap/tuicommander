import { createRoot } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeTerminal } from "../helpers/store";

const playQuestion = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock("../../stores/notifications", () => ({ notificationsStore: { playQuestion } }));

describe("trackQuestionReminder", () => {
	let store: typeof import("../../stores/terminals").terminalsStore;
	let QUESTION_REMINDER_MS: number;
	let dispose: () => void;
	let id: string;

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		playQuestion.mockClear();
		store = (await import("../../stores/terminals")).terminalsStore;
		const mod = await import("../../components/Terminal/questionReminder");
		QUESTION_REMINDER_MS = mod.QUESTION_REMINDER_MS;
		createRoot((d) => {
			dispose = d;
			id = store.add(makeTerminal());
			mod.trackQuestionReminder(id);
		});
	});

	afterEach(() => {
		dispose();
		store._testCancelPendingTimers();
		vi.useRealTimers();
	});

	it("re-notifies once when the question is still pending after 120 s (bug: notified only once)", () => {
		store.setAwaitingInput(id, "question", true);
		vi.advanceTimersByTime(QUESTION_REMINDER_MS - 1);
		expect(playQuestion).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		expect(playQuestion).toHaveBeenCalledExactlyOnceWith(id);
		// one reminder per question: no repeat every 2 minutes
		vi.advanceTimersByTime(QUESTION_REMINDER_MS * 5);
		expect(playQuestion).toHaveBeenCalledTimes(1);
	});

	it("is cancelled when the question is answered or the agent resumes (state cleared)", () => {
		store.setAwaitingInput(id, "question", true);
		vi.advanceTimersByTime(QUESTION_REMINDER_MS - 1000);
		store.clearAwaitingInput(id);
		vi.advanceTimersByTime(QUESTION_REMINDER_MS * 2);
		expect(playQuestion).not.toHaveBeenCalled();
	});

	it("is cancelled when the pending state turns into an error", () => {
		store.setAwaitingInput(id, "question", true);
		store.setAwaitingInput(id, "error", true);
		vi.advanceTimersByTime(QUESTION_REMINDER_MS * 2);
		expect(playQuestion).not.toHaveBeenCalled();
	});

	it("is cancelled when the session is closed (bug: reminder for a removed terminal)", () => {
		store.setAwaitingInput(id, "question", true);
		store.remove(id);
		vi.advanceTimersByTime(QUESTION_REMINDER_MS * 2);
		expect(playQuestion).not.toHaveBeenCalled();
	});

	it("is cancelled when the component is disposed", () => {
		store.setAwaitingInput(id, "question", true);
		dispose();
		vi.advanceTimersByTime(QUESTION_REMINDER_MS * 2);
		expect(playQuestion).not.toHaveBeenCalled();
	});

	it("a new question restarts the timer from zero", () => {
		store.setAwaitingInput(id, "question", true);
		vi.advanceTimersByTime(QUESTION_REMINDER_MS - 1000);
		store.clearAwaitingInput(id);
		store.setAwaitingInput(id, "question", true);
		vi.advanceTimersByTime(QUESTION_REMINDER_MS - 1);
		expect(playQuestion).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		expect(playQuestion).toHaveBeenCalledTimes(1);
	});
});
