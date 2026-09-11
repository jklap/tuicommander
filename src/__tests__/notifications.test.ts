import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { isNotificationSound, NOTIFICATION_SOUNDS, type NotificationConfig } from "../notifications";

// Mock the invoke module — NotificationManager delegates to Rust
vi.mock("../invoke", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
}));

describe("NotificationManager", () => {
	let NotificationManager: typeof import("../notifications").NotificationManager;
	let manager: InstanceType<typeof NotificationManager>;
	let mockInvoke: ReturnType<typeof vi.fn>;

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.advanceTimersByTime(1000);
		vi.resetModules();

		const mod = await import("../notifications");
		NotificationManager = mod.NotificationManager;
		manager = new NotificationManager();

		const invokeModule = await import("../invoke");
		mockInvoke = invokeModule.invoke as ReturnType<typeof vi.fn>;
		mockInvoke.mockClear();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	describe("constructor", () => {
		it("uses defaults", () => {
			const config = manager.getConfig();
			expect(config.enabled).toBe(true);
			expect(config.volume).toBe(0.5);
		});

		it("merges partial config", () => {
			const m = new NotificationManager({ volume: 0.8 });
			expect(m.getConfig().volume).toBe(0.8);
			expect(m.getConfig().enabled).toBe(true);
		});
	});

	describe("play()", () => {
		it("calls Rust play_notification_sound when enabled", async () => {
			await manager.play("question");
			expect(mockInvoke).toHaveBeenCalledWith("play_notification_sound", {
				sound: "question",
				volume: 0.5,
				device: null,
				choice: { preset: "default", custom_path: null },
			});
		});

		it("does nothing when disabled", async () => {
			manager.setEnabled(false);
			await manager.play("question");
			expect(mockInvoke).not.toHaveBeenCalled();
		});

		it("does nothing when specific sound is disabled", async () => {
			manager.setSoundEnabled("question", false);
			await manager.play("question");
			expect(mockInvoke).not.toHaveBeenCalled();
		});

		it("rate-limits rapid plays of same sound", async () => {
			await manager.play("question");
			await manager.play("question");
			// Only first call should go through
			expect(mockInvoke).toHaveBeenCalledTimes(1);
		});

		it("allows play after rate limit interval", async () => {
			await manager.play("question");
			expect(mockInvoke).toHaveBeenCalledTimes(1);
			// Advance past the 500ms rate limit
			await vi.advanceTimersByTimeAsync(600);
			await manager.play("question");
			expect(mockInvoke).toHaveBeenCalledTimes(2);
		});

		it("force bypasses the rate limit (rapid A/B test clicks all play)", async () => {
			await manager.play("question", { force: true });
			await manager.play("question", { force: true });
			expect(mockInvoke).toHaveBeenCalledTimes(2);
		});

		it("force plays even when notifications are disabled", async () => {
			manager.setEnabled(false);
			await manager.play("question", { force: true });
			expect(mockInvoke).toHaveBeenCalledTimes(1);
		});

		it("force plays even when the specific sound is disabled", async () => {
			manager.setSoundEnabled("question", false);
			await manager.play("question", { force: true });
			expect(mockInvoke).toHaveBeenCalledTimes(1);
		});

		it("force uses the current volume", async () => {
			manager.setVolume(0.8);
			await manager.play("completion", { force: true });
			expect(mockInvoke).toHaveBeenCalledWith("play_notification_sound", {
				sound: "completion",
				volume: 0.8,
				device: null,
				choice: { preset: "default", custom_path: null },
			});
		});

		it("handles invoke error gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("audio error"));
			// Should not throw
			await expect(manager.play("question")).resolves.toBeUndefined();
		});

		it("passes configured volume to Rust", async () => {
			manager.setVolume(0.8);
			await manager.play("completion");
			expect(mockInvoke).toHaveBeenCalledWith("play_notification_sound", {
				sound: "completion",
				volume: 0.8,
				device: null,
				choice: { preset: "default", custom_path: null },
			});
		});

		it("forwards a configured audio_device to Rust", async () => {
			manager.updateConfig({ audio_device: "USB Speakers" });
			await manager.play("question");
			expect(mockInvoke).toHaveBeenCalledWith("play_notification_sound", {
				sound: "question",
				volume: 0.5,
				device: "USB Speakers",
				choice: { preset: "default", custom_path: null },
			});
		});
	});

	describe("exponential backoff", () => {
		it("backs off after 3 consecutive failures, then recovers once the window passes", async () => {
			mockInvoke
				.mockRejectedValueOnce(new Error("e1"))
				.mockRejectedValueOnce(new Error("e2"))
				.mockRejectedValueOnce(new Error("e3"));

			await manager.play("question");
			expect(mockInvoke).toHaveBeenCalledTimes(1);

			await vi.advanceTimersByTimeAsync(600);
			await manager.play("question");
			expect(mockInvoke).toHaveBeenCalledTimes(2);

			await vi.advanceTimersByTimeAsync(600);
			await manager.play("question");
			expect(mockInvoke).toHaveBeenCalledTimes(3); // 3rd failure arms the 5s backoff

			// Still well within the 5s backoff window — invoke must not even be attempted.
			await vi.advanceTimersByTimeAsync(600);
			await manager.play("question");
			expect(mockInvoke).toHaveBeenCalledTimes(3);

			// Past the 5s window — resumes attempting.
			await vi.advanceTimersByTimeAsync(4500);
			await manager.play("question");
			expect(mockInvoke).toHaveBeenCalledTimes(4);
		});

		it("resets the failure count after a successful play, so a later run of failures re-arms from 3, not cumulatively", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("e1")).mockRejectedValueOnce(new Error("e2"));

			await manager.play("question"); // failure 1
			await vi.advanceTimersByTimeAsync(600);
			await manager.play("question"); // failure 2 — still below the 3-failure threshold
			await vi.advanceTimersByTimeAsync(600);
			await manager.play("question"); // succeeds (default mock), resets consecutiveFailures to 0
			expect(mockInvoke).toHaveBeenCalledTimes(3);

			mockInvoke
				.mockRejectedValueOnce(new Error("e4"))
				.mockRejectedValueOnce(new Error("e5"))
				.mockRejectedValueOnce(new Error("e6"));
			await vi.advanceTimersByTimeAsync(600);
			await manager.play("question");
			await vi.advanceTimersByTimeAsync(600);
			await manager.play("question");
			await vi.advanceTimersByTimeAsync(600);
			await manager.play("question"); // 3rd fresh failure — arms backoff again
			expect(mockInvoke).toHaveBeenCalledTimes(6);

			await vi.advanceTimersByTimeAsync(600);
			await manager.play("question");
			expect(mockInvoke).toHaveBeenCalledTimes(6); // backed off, not a 7th attempt
		});
	});

	describe("convenience play methods", () => {
		it("playQuestion delegates to play", async () => {
			const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
			await manager.playQuestion();
			expect(spy).toHaveBeenCalledWith("question");
		});

		it("playError delegates to play", async () => {
			const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
			await manager.playError();
			expect(spy).toHaveBeenCalledWith("error");
		});

		it("playCompletion delegates to play", async () => {
			const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
			await manager.playCompletion();
			expect(spy).toHaveBeenCalledWith("completion");
		});

		it("playWarning delegates to play", async () => {
			const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
			await manager.playWarning();
			expect(spy).toHaveBeenCalledWith("warning");
		});

		it("playInfo delegates to play", async () => {
			const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
			await manager.playInfo();
			expect(spy).toHaveBeenCalledWith("info");
		});
	});

	describe("configuration methods", () => {
		it("setEnabled updates enabled state", () => {
			manager.setEnabled(false);
			expect(manager.getConfig().enabled).toBe(false);
		});

		it("setVolume clamps to 0-1", () => {
			manager.setVolume(1.5);
			expect(manager.getConfig().volume).toBe(1);
			manager.setVolume(-0.5);
			expect(manager.getConfig().volume).toBe(0);
		});

		it("setSoundEnabled updates specific sound", () => {
			manager.setSoundEnabled("question", false);
			expect(manager.getConfig().sounds.question).toBe(false);
		});

		it("setSoundChoice updates only the given sound's choice", () => {
			manager.setSoundChoice("question", { preset: "attention", custom_path: null });
			expect(manager.getConfig().sound_choices.question).toEqual({ preset: "attention", custom_path: null });
			expect(manager.getConfig().sound_choices.error).toEqual({ preset: "default", custom_path: null });
		});

		it("setSoundChoice's choice is forwarded on the next play()", async () => {
			manager.setSoundChoice("error", { preset: "custom", custom_path: "/Users/me/ding.wav" });
			await manager.play("error");
			expect(mockInvoke).toHaveBeenCalledWith("play_notification_sound", {
				sound: "error",
				volume: 0.5,
				device: null,
				choice: { preset: "custom", custom_path: "/Users/me/ding.wav" },
			});
		});

		it("updateConfig merges config", () => {
			manager.updateConfig({ volume: 0.9, enabled: false });
			expect(manager.getConfig().volume).toBe(0.9);
			expect(manager.getConfig().enabled).toBe(false);
		});

		it("updateConfig shallow-merges: a partial `sounds` patch replaces the whole map, not just the given keys", () => {
			// Documents current behavior as a guardrail. `NotificationConfig["sounds"]` is
			// currently a flat Record<NotificationSound, boolean> so no real caller passes a
			// partial `sounds` object today — but a future per-sound field (e.g. a custom
			// sound-source override) is exactly the kind of nested value this shallow merge
			// would silently corrupt. If this test starts failing because updateConfig grew
			// a deep merge, that's fine — update it; the point is a future regression here
			// must be a deliberate change, not a silent one.
			manager.updateConfig({ sounds: { question: false } as unknown as NotificationConfig["sounds"] });
			const sounds = manager.getConfig().sounds;
			expect(sounds.question).toBe(false);
			expect(sounds.error).toBeUndefined();
			expect(sounds.completion).toBeUndefined();
		});
	});

	describe("isNotificationSound()", () => {
		it("accepts every known sound name", () => {
			for (const sound of NOTIFICATION_SOUNDS) {
				expect(isNotificationSound(sound)).toBe(true);
			}
		});

		it("rejects unknown strings", () => {
			expect(isNotificationSound("bogus")).toBe(false);
			expect(isNotificationSound("")).toBe(false);
			expect(isNotificationSound("Question")).toBe(false); // case-sensitive
		});

		it("rejects non-string values", () => {
			expect(isNotificationSound(null)).toBe(false);
			expect(isNotificationSound(undefined)).toBe(false);
			expect(isNotificationSound(42)).toBe(false);
			expect(isNotificationSound({})).toBe(false);
			expect(isNotificationSound(["question"])).toBe(false);
		});
	});

	describe("isAvailable()", () => {
		it("always returns true (native audio via Rust)", () => {
			expect(manager.isAvailable()).toBe(true);
		});
	});
});

describe("DEFAULT_NOTIFICATION_CONFIG", () => {
	it("has expected defaults", async () => {
		vi.resetModules();
		const mod = await import("../notifications");
		expect(mod.DEFAULT_NOTIFICATION_CONFIG.enabled).toBe(true);
		expect(mod.DEFAULT_NOTIFICATION_CONFIG.volume).toBe(0.5);
		expect(mod.DEFAULT_NOTIFICATION_CONFIG.sounds.question).toBe(true);
		expect(mod.DEFAULT_NOTIFICATION_CONFIG.sounds.error).toBe(true);
		expect(mod.DEFAULT_NOTIFICATION_CONFIG.sounds.completion).toBe(true);
		expect(mod.DEFAULT_NOTIFICATION_CONFIG.sounds.warning).toBe(true);
	});
});

describe("browser attention sound", () => {
	const originalTauriInternals = (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
	const originalTauriShim = (globalThis as Record<string, unknown>).__TAURI_SHIM__;

	afterEach(() => {
		(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = originalTauriInternals;
		if (originalTauriShim === undefined) {
			delete (globalThis as Record<string, unknown>).__TAURI_SHIM__;
		} else {
			(globalThis as Record<string, unknown>).__TAURI_SHIM__ = originalTauriShim;
		}
		vi.unstubAllGlobals();
	});

	it("schedules the approved G4-G4-E5 double-knock motif", async () => {
		(globalThis as Record<string, unknown>).__TAURI_SHIM__ = true;
		const oscillators = Array.from({ length: 3 }, () => ({
			type: "sine" as OscillatorType,
			frequency: { value: 0 },
			connect: vi.fn(),
			disconnect: vi.fn(),
			start: vi.fn(),
			stop: vi.fn(),
			onended: null as (() => void) | null,
		}));
		let oscillatorIndex = 0;
		vi.stubGlobal(
			"AudioContext",
			class {
				currentTime = 10;
				state = "running";
				destination = {};
				resume = vi.fn();
				createGain = () => ({ gain: { value: 0 }, connect: vi.fn() });
				createOscillator = () => oscillators[oscillatorIndex++];
			},
		);
		vi.resetModules();

		const { NotificationManager } = await import("../notifications");
		await new NotificationManager().play("attention");

		const starts = oscillators.map((oscillator) => oscillator.start.mock.calls[0][0]);
		const stops = oscillators.map((oscillator) => oscillator.stop.mock.calls[0][0]);
		expect(starts.map((time) => Math.round((time - 10) * 1000))).toEqual([0, 125, 250]);
		expect(stops.map((time) => Math.round((time - 10) * 1000))).toEqual([75, 200, 390]);
		expect(starts.slice(1).map((time, index) => Math.round((time - stops[index]) * 1000))).toEqual([50, 50]);
		expect(oscillators.map((oscillator) => oscillator.frequency.value)).toEqual([392, 392, 659]);
		expect(oscillators.map((oscillator) => oscillator.type)).toEqual(["triangle", "triangle", "triangle"]);
	});
});

describe("browser preset resolution (Web Audio fallback)", () => {
	const originalTauriInternals = (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
	const originalTauriShim = (globalThis as Record<string, unknown>).__TAURI_SHIM__;

	afterEach(() => {
		(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = originalTauriInternals;
		if (originalTauriShim === undefined) {
			delete (globalThis as Record<string, unknown>).__TAURI_SHIM__;
		} else {
			(globalThis as Record<string, unknown>).__TAURI_SHIM__ = originalTauriShim;
		}
		vi.unstubAllGlobals();
	});

	function stubAudioContext() {
		const oscillators: Array<{ frequency: { value: number }; type: OscillatorType }> = [];
		vi.stubGlobal(
			"AudioContext",
			class {
				currentTime = 0;
				state = "running";
				destination = {};
				resume = vi.fn();
				createGain = () => ({ gain: { value: 0 }, connect: vi.fn() });
				createOscillator = () => {
					const osc = {
						frequency: { value: 0 },
						type: "sine" as OscillatorType,
						connect: vi.fn(),
						start: vi.fn(),
						stop: vi.fn(),
						onended: null as (() => void) | null,
					};
					oscillators.push(osc);
					return osc;
				};
			},
		);
		return oscillators;
	}

	it("a preset borrowed from another sound plays THAT sound's tone frequencies", async () => {
		(globalThis as Record<string, unknown>).__TAURI_SHIM__ = true;
		const oscillators = stubAudioContext();
		vi.resetModules();

		const { NotificationManager } = await import("../notifications");
		const manager = new NotificationManager();
		manager.setSoundChoice("question", { preset: "completion", custom_path: null });
		await manager.play("question");

		// "completion"'s own tone is [660, 880] — distinct from "question"'s own [880, 1100].
		expect(oscillators.map((o) => o.frequency.value)).toEqual([660, 880]);
	});

	it("'custom' has no meaning in the browser — falls back to the sound's own tone", async () => {
		(globalThis as Record<string, unknown>).__TAURI_SHIM__ = true;
		const oscillators = stubAudioContext();
		vi.resetModules();

		const { NotificationManager } = await import("../notifications");
		const manager = new NotificationManager();
		manager.setSoundChoice("question", { preset: "custom", custom_path: "/some/file.wav" });
		await manager.play("question");

		expect(oscillators.map((o) => o.frequency.value)).toEqual([880, 1100]);
	});

	it("'default' plays the sound's own tone, unchanged", async () => {
		(globalThis as Record<string, unknown>).__TAURI_SHIM__ = true;
		const oscillators = stubAudioContext();
		vi.resetModules();

		const { NotificationManager } = await import("../notifications");
		const manager = new NotificationManager();
		await manager.play("error");

		expect(oscillators.map((o) => o.frequency.value)).toEqual([440, 330]);
	});
});
