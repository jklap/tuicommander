import { invoke } from "./invoke";
import { appLogger } from "./stores/appLogger";
import { isTauri } from "./transport";

/** Notification sound types */
export type NotificationSound = "question" | "error" | "completion" | "warning" | "info" | "attention";

/** Every sound name, for runtime validation of values coming off the wire. */
export const NOTIFICATION_SOUNDS: readonly NotificationSound[] = [
	"question",
	"error",
	"completion",
	"warning",
	"info",
	"attention",
] as const;

export function isNotificationSound(value: unknown): value is NotificationSound {
	return typeof value === "string" && (NOTIFICATION_SOUNDS as readonly string[]).includes(value);
}

/** A sound source a user can pick for one notification type: the built-in
 *  default tone, another notification type's tone borrowed as a preset, or a
 *  user-supplied audio file (desktop only — see `SoundChoice.custom_path`). */
export type SoundPreset = "default" | NotificationSound | "custom";

export const SOUND_PRESETS: readonly SoundPreset[] = [
	"default",
	"question",
	"completion",
	"error",
	"warning",
	"info",
	"attention",
	"custom",
] as const;

export function isSoundPreset(value: unknown): value is SoundPreset {
	return typeof value === "string" && (SOUND_PRESETS as readonly string[]).includes(value);
}

/** The chosen sound source for one notification type. `custom_path` is only
 *  meaningful when `preset === "custom"`; it's a native filesystem path, so it
 *  only ever plays in Tauri mode (the browser/PWA fallback has no filesystem
 *  access and falls back to the sound's own default tone). */
export interface SoundChoice {
	preset: SoundPreset;
	custom_path: string | null;
}

function defaultSoundChoice(): SoundChoice {
	return { preset: "default", custom_path: null };
}

/** Notification configuration */
export interface NotificationConfig {
	enabled: boolean;
	volume: number; // 0.0 to 1.0
	sounds: Record<NotificationSound, boolean>;
	audio_device: string | null;
	/** Per-sound source selection. Independent of `sounds` above, which only
	 *  gates whether that sound plays at all. */
	sound_choices: Record<NotificationSound, SoundChoice>;
	/** Drop the completion chime for MCP/HTTP-created sessions (`session create`,
	 *  `agent spawn`) — an orchestration of many agents otherwise beeps per worker.
	 *  Visual signals (activity item, badge, OS notification) are unaffected. */
	silence_remote_completions: boolean;
	/** Mirror every toast into the toolbar bell, so a message that auto-dismissed
	 *  while the user looked elsewhere is still readable afterwards. */
	toasts_in_bell: boolean;
}

/** Default notification configuration */
export const DEFAULT_NOTIFICATION_CONFIG: NotificationConfig = {
	enabled: true,
	volume: 0.5,
	sounds: {
		question: true,
		error: true,
		completion: true,
		warning: true,
		info: true,
		attention: true,
	},
	audio_device: null,
	sound_choices: {
		question: defaultSoundChoice(),
		error: defaultSoundChoice(),
		completion: defaultSoundChoice(),
		warning: defaultSoundChoice(),
		info: defaultSoundChoice(),
		attention: defaultSoundChoice(),
	},
	silence_remote_completions: true,
	toasts_in_bell: true,
};

/** Notification manager — delegates audio playback to Rust via Tauri IPC.
 *  Handles config, per-sound enable/disable, and rate limiting in JS.
 *  Actual tone generation happens natively (rodio), bypassing WebKit
 *  AudioContext restrictions entirely. */
export class NotificationManager {
	private config: NotificationConfig;
	private lastPlayTime: Map<NotificationSound, number> = new Map();
	private readonly minInterval = 500; // Minimum ms between same sound
	private consecutiveFailures = 0;
	private backoffUntil = 0;

	constructor(config: Partial<NotificationConfig> = {}) {
		this.config = { ...DEFAULT_NOTIFICATION_CONFIG, ...config };
	}

	/** Play a notification sound.
	 *  `force` bypasses the enabled / per-sound / backoff / rate-limit gates — used
	 *  by the Settings "Test" buttons, where the user explicitly asked to hear the
	 *  sound right now and throttling would silently swallow rapid A/B comparisons. */
	async play(sound: NotificationSound, opts?: { force?: boolean }): Promise<void> {
		const now = Date.now();

		if (!opts?.force) {
			if (!this.config.enabled) return;
			if (!this.config.sounds[sound]) return;

			// Back off after repeated failures (exponential: 5s, 30s, 5min cap)
			if (now < this.backoffUntil) return;

			// Rate limit: prevent spam
			const lastPlay = this.lastPlayTime.get(sound) || 0;
			if (now - lastPlay < this.minInterval) return;
			this.lastPlayTime.set(sound, now);
		}

		try {
			const choice = this.config.sound_choices[sound];
			if (isTauri()) {
				await invoke("play_notification_sound", {
					sound,
					volume: this.config.volume,
					device: this.config.audio_device,
					choice,
				});
			} else {
				playWebAudioTone(sound, this.config.volume, choice);
			}
			this.consecutiveFailures = 0;
		} catch (err) {
			this.consecutiveFailures++;
			appLogger.debug("app", `Notification sound failed (attempt ${this.consecutiveFailures})`, err);
			if (this.consecutiveFailures >= 3) {
				const delay = Math.min(300_000, 5_000 * 2 ** (this.consecutiveFailures - 3));
				this.backoffUntil = now + delay;
				appLogger.warn("app", `Notification sound backing off ${Math.round(delay / 1000)}s`, err);
			}
		}
	}

	async playQuestion(): Promise<void> {
		return this.play("question");
	}
	async playError(): Promise<void> {
		return this.play("error");
	}
	async playCompletion(): Promise<void> {
		return this.play("completion");
	}
	async playWarning(): Promise<void> {
		return this.play("warning");
	}
	async playInfo(): Promise<void> {
		return this.play("info");
	}

	updateConfig(config: Partial<NotificationConfig>): void {
		this.config = { ...this.config, ...config };
	}

	setEnabled(enabled: boolean): void {
		this.config.enabled = enabled;
	}

	setVolume(volume: number): void {
		this.config.volume = Math.max(0, Math.min(1, volume));
	}

	setSoundEnabled(sound: NotificationSound, enabled: boolean): void {
		this.config.sounds[sound] = enabled;
	}

	setSoundChoice(sound: NotificationSound, choice: SoundChoice): void {
		this.config.sound_choices[sound] = choice;
	}

	getConfig(): NotificationConfig {
		return { ...this.config };
	}

	/** Notifications are always available when running in Tauri (native audio) */
	isAvailable(): boolean {
		return true;
	}
}

// Web Audio API fallback for browser mode — simple tones per sound type
const TONE_FREQS: Record<NotificationSound, number[]> = {
	question: [880, 1100], // ascending two-tone
	error: [440, 330], // descending two-tone
	completion: [660, 880], // ascending two-tone
	warning: [550, 440], // descending two-tone
	info: [660], // single tone
	attention: [392, 392, 659], // two quick G4 knocks, then a longer E5 call
};

/** Timbre per sound. The attention call uses a softer triangle wave while its
 *  double-knock motif keeps it distinct from the chimes. Mirrors Rust. */
const TONE_WAVEFORMS: Partial<Record<NotificationSound, OscillatorType>> = {
	attention: "triangle",
};

/** Extra attenuation per sound, mirroring the Rust engine. */
const TONE_GAINS: Partial<Record<NotificationSound, number>> = {
	attention: 0.8,
};

const ATTENTION_DURATIONS = [0.075, 0.075, 0.14];

let webAudioCtx: AudioContext | null = null;

/** Resolve a chosen preset to the sound whose tone table entry should
 *  actually play. "custom" has no meaning in browser mode — there's no
 *  filesystem access to a native path — so it falls back to the sound's own
 *  default tone, same as an unrecognized preset would. */
function resolveToneSound(sound: NotificationSound, choice: SoundChoice): NotificationSound {
	if (choice.preset === "default" || choice.preset === "custom") return sound;
	return choice.preset;
}

function playWebAudioTone(sound: NotificationSound, volume: number, choice: SoundChoice): void {
	if (!webAudioCtx) {
		webAudioCtx = new AudioContext();
	}
	const ctx = webAudioCtx;
	if (ctx.state === "suspended") ctx.resume();

	const toneSound = resolveToneSound(sound, choice);
	const freqs = TONE_FREQS[toneSound] ?? [660];
	const gain = ctx.createGain();
	gain.gain.value = volume * 0.3 * (TONE_GAINS[toneSound] ?? 1); // Gentle volume
	gain.connect(ctx.destination);
	const interval = toneSound === "attention" ? 0.125 : 0.15;

	freqs.forEach((freq, i) => {
		const duration = toneSound === "attention" ? ATTENTION_DURATIONS[i] : 0.12;
		const osc = ctx.createOscillator();
		osc.type = TONE_WAVEFORMS[toneSound] ?? "sine";
		osc.frequency.value = freq;
		osc.connect(gain);
		osc.start(ctx.currentTime + i * interval);
		osc.stop(ctx.currentTime + i * interval + duration);
		osc.onended = () => osc.disconnect();
	});
}

/** Global notification manager instance */
export const notificationManager = new NotificationManager();
