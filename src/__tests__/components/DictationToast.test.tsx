import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScopeAsync } from "../helpers/store";
// Must import mocks before store/component
import { mockInvoke } from "../mocks/tauri";

describe("DictationToast", () => {
	let DictationToast: typeof import("../../components/DictationToast/DictationToast").DictationToast;
	let dictationStore: typeof import("../../stores/dictation").dictationStore;
	let easeMeterLevel: typeof import("../../stores/dictation").easeMeterLevel;

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
		const storeModule = await import("../../stores/dictation");
		dictationStore = storeModule.dictationStore;
		easeMeterLevel = storeModule.easeMeterLevel;
		const component = await import("../../components/DictationToast/DictationToast");
		DictationToast = component.DictationToast;
	});

	it("is hidden by default", () => {
		const { container } = render(() => <DictationToast />);
		expect(container.querySelector(".toast")).toBeNull();
	});

	it("shows the live preview and an idle meter when recording starts", async () => {
		// Mock start_dictation to succeed
		mockInvoke.mockResolvedValueOnce(undefined);

		await testInScopeAsync(async () => {
			const { container } = render(() => <DictationToast />);

			// Start recording (sets recording=true in store)
			await dictationStore.startRecording();
			expect(dictationStore.state.recording).toBe(true);

			expect(container.querySelector(".toast")).not.toBeNull();
			expect(container.querySelector('[role="meter"]')?.getAttribute("aria-valuenow")).toBe("0");
			// Push-to-talk is a short take, so its pulse and dots stay.
			expect(container.querySelector(".indicator")).not.toBeNull();
			expect(container.querySelector(".dots")).not.toBeNull();
			expect(container.querySelector(".voiceMeter")).toBeNull();

			mockInvoke.mockResolvedValueOnce({ text: "", skip_reason: "no speech detected", duration_s: 0, truncated_s: 0 });
			await dictationStore.stopRecording();
		});
	});

	it("hides toast after recording stops", async () => {
		mockInvoke.mockResolvedValueOnce(undefined); // start_dictation

		await testInScopeAsync(async () => {
			render(() => <DictationToast />);

			await dictationStore.startRecording();
			expect(dictationStore.state.recording).toBe(true);

			// Stop recording
			mockInvoke.mockResolvedValueOnce({ text: "hello", skip_reason: null, duration_s: 1.0, truncated_s: 0 });
			await dictationStore.stopRecording();

			expect(dictationStore.state.recording).toBe(false);
			expect(dictationStore.state.partialText).toBe("");
		});
	});

	describe("while a hands-free conversation is armed", () => {
		const status = (armed: boolean, phase: string) => ({
			armed,
			phase,
			sessionId: armed ? "s1" : null,
			owner: armed ? "desktop" : null,
			generation: 1,
			pendingText: null,
			queuedIds: [],
			holdBackMs: 1500,
			error: null,
		});

		/** Answer each command the arm path and the monitor issue. */
		const backend = (hands: { armed: boolean; phase: string }, level: number, speaking = false) =>
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "arm_hands_free_dictation" || cmd === "get_hands_free_status") {
					return Promise.resolve(status(hands.armed, hands.phase));
				}
				if (cmd === "disarm_hands_free_dictation") {
					return Promise.resolve({ alreadyDelivered: [], status: status(false, "disarmed") });
				}
				if (cmd === "get_dictation_status") return Promise.resolve({ audio_level: level });
				if (cmd === "get_speech_status") return Promise.resolve({ speaking });
				return Promise.resolve(undefined);
			});

		afterEach(() => {
			vi.useRealTimers();
		});

		it("shows the phase and the live microphone level without push-to-talk", async () => {
			vi.useFakeTimers();
			backend({ armed: true, phase: "waiting" }, 0.5);

			await testInScopeAsync(async () => {
				const { container } = render(() => <DictationToast />);
				expect(await dictationStore.armHandsFree("s1")).toBe(true);
				expect(dictationStore.state.recording).toBe(false);

				await vi.advanceTimersByTimeAsync(100);
				expect(container.querySelector(".toast")).not.toBeNull();
				expect(container.textContent).toContain("Listening");
				// The raw 0.5 reaches the meter through the noise floor, not as 50%.
				const eased = Math.round(easeMeterLevel(0, 0.5) * 100);
				expect(eased).toBeGreaterThan(0);
				expect(eased).toBeLessThan(50);
				const meter = container.querySelector('[role="meter"]');
				expect(meter?.classList.contains("voiceMeter")).toBe(true);
				expect(meter?.getAttribute("aria-valuenow")).toBe(String(eased));
				// A conversation stays open for minutes: nothing may pulse or animate.
				expect(container.querySelector(".indicator")).toBeNull();
				expect(container.querySelector(".dots")).toBeNull();

				await dictationStore.disarmHandsFree();
			});
		});

		it("follows the phase and says when the reply is being spoken", async () => {
			vi.useFakeTimers();
			const hands = { armed: true, phase: "waiting" };
			backend(hands, 0.1);

			await testInScopeAsync(async () => {
				const { container } = render(() => <DictationToast />);
				await dictationStore.armHandsFree("s1");

				hands.phase = "transcribing";
				await vi.advanceTimersByTimeAsync(400);
				expect(container.textContent).toContain("Transcribing");

				backend(hands, 0.1, true);
				await vi.advanceTimersByTimeAsync(400);
				expect(container.textContent).toContain("Speaking");

				await dictationStore.disarmHandsFree();
			});
		});

		it("hides and stops polling once the conversation is disarmed", async () => {
			vi.useFakeTimers();
			backend({ armed: true, phase: "waiting" }, 0.5);

			await testInScopeAsync(async () => {
				const { container } = render(() => <DictationToast />);
				await dictationStore.armHandsFree("s1");
				await vi.advanceTimersByTimeAsync(100);
				expect(container.querySelector(".toast")).not.toBeNull();

				await dictationStore.disarmHandsFree();
				expect(dictationStore.state.audioLevel).toBe(0);
				await vi.advanceTimersByTimeAsync(200);
				expect(container.querySelector(".toast")).toBeNull();

				mockInvoke.mockClear();
				await vi.advanceTimersByTimeAsync(1000);
				expect(mockInvoke).not.toHaveBeenCalledWith("get_dictation_status");
			});
		});
	});
});
