import { createStore } from "solid-js/store";
import { invoke, listen } from "../invoke";
import { isTauri } from "../transport";
import { appLogger } from "./appLogger";

/** Dictation config persisted to ~/.tuicommander/dictation-config.json */
interface DictationConfig {
	enabled: boolean;
	hotkey: string;
	language: string;
	model: string;
	device: string | null;
	long_press_ms: number;
	auto_send: boolean;
	rms_threshold: number;
	no_speech_threshold: number;
	/** Hands-free hold-back before a transcript is enqueued. */
	hands_free_hold_back_ms: number;
	/** Hands-free activation phrase; empty means ungated. */
	hands_free_activation_phrase: string;
	/** Tell the bound model when hands-free starts and stops. On by default. */
	hands_free_notify_model: boolean;
	/** A user-supplied speech engine as argv; empty means the bundled one. No UI control. */
	speech_command: string[];
	/** Which of the language's voices speaks. Empty means the first one it ships. */
	speech_voice: string;
}

/**
 * A downloadable speech asset: the ONNX runtime, or one language bundle.
 *
 * Snake_case because `SpeechAssetInfo` carries no serde rename, unlike the
 * hands-free and speech status structs below it.
 */
export interface SpeechAsset {
	id: string;
	display_name: string;
	/** `"language"` or `"runtime"`. */
	kind: string;
	/**
	 * The Whisper language code this speaks (`"it"`); null for the runtime
	 * library. The same alphabet as `DictationConfig.language`, so the two can
	 * be compared directly — which is the whole reason it is a code.
	 */
	language: string | null;
	voices: string[];
	download_bytes: number;
	/** `"absent"`, `"downloading"`, `"incomplete"` or `"ready"`. */
	state: string;
	/** Which files an incomplete asset is missing. Empty otherwise. */
	missing: string[];
}

/** What the hands-free conversation is doing. Mirrors `Phase::as_wire`. */
export type HandsFreePhase =
	| "disarmed"
	| "waiting"
	| "capturing"
	| "transcribing"
	| "holding_back"
	| "delivered"
	| "error";

/** Live hands-free state, mirroring Rust's `HandsFreeStatus`. */
export interface HandsFreeStatus {
	armed: boolean;
	phase: HandsFreePhase;
	/** The bound terminal. Unchanged by focus for as long as it is set. */
	sessionId: string | null;
	/** The bound audio endpoint. */
	owner: string | null;
	generation: number;
	/** The transcript waiting out its hold-back, while there is still time to stop it. */
	pendingText: string | null;
	queuedIds: number[];
	holdBackMs: number;
	error: string | null;
}

/** Whether a reply can be spoken, and what the speaker is doing. */
export interface SpeechStatus {
	available: boolean;
	unavailableReason: string;
	sessionId: string | null;
	language: string;
	turn: number;
	voice: string;
	queued: number;
	/** Synthesis is running. */
	rendering: boolean;
	/** Audio is playing. */
	speaking: boolean;
	lastError: string | null;
}

/**
 * The only audio endpoint this build serves.
 *
 * Rust refuses any other owner rather than opening the microphone on the
 * machine running TUICommander, so a browser tab cannot arm — see
 * `DESKTOP_OWNER` in `dictation/commands.rs`.
 */
export const DESKTOP_AUDIO_OWNER = "desktop";

/** Whisper's own no_speech_thold default, mirrored from `transcribe.rs`. */
export const DEFAULT_NO_SPEECH_THRESHOLD = 0.6;

/** The historical hardcoded RMS floor, mirrored from `transcribe.rs`. */
export const DEFAULT_RMS_THRESHOLD = 0.001;

/**
 * The hands-free hold-back default, mirrored from `default_hold_back_ms`.
 *
 * Long enough to read a transcript and stop it, short enough not to feel like
 * a delay. Only where the slider starts — Rust owns the number that is used.
 */
export const DEFAULT_HOLD_BACK_MS = 1500;

/** GPU/CPU backend reported by whisper after model load. */
export type DictationBackend = "cpu" | "gpu";

/** Model info from Rust backend */
export interface ModelInfo {
	name: string;
	display_name: string;
	size_hint_mb: number;
	downloaded: boolean;
	actual_size_mb: number;
}

/** Model status values from Rust backend */
type ModelStatus = "not_downloaded" | "downloaded" | "ready";

/** Model status from Rust backend */
interface DictationStatus {
	model_status: ModelStatus;
	model_name: string;
	model_size_mb: number;
	recording: boolean;
	processing: boolean;
	audio_level?: number;
}

/** Transcription response from Rust backend */
interface TranscribeResponse {
	text: string;
	skip_reason: string | null;
	duration_s: number;
	/** Seconds of speech the recording cap dropped — 0 for an ordinary recording. */
	truncated_s: number;
}

/** Audio device from Rust backend */
interface AudioDevice {
	name: string;
	is_default: boolean;
}

/** Download progress event payload */
interface DownloadProgress {
	downloaded: number;
	total: number;
	percent: number;
}

function normalizeAudioLevel(value: number | undefined): number {
	return Number.isFinite(value) ? Math.max(0, Math.min(1, value as number)) : 0;
}

/** Supported languages for Whisper */
export const WHISPER_LANGUAGES: Record<string, string> = {
	auto: "Auto-detect",
	en: "English",
	es: "Spanish",
	fr: "French",
	de: "German",
	it: "Italian",
	pt: "Portuguese",
	nl: "Dutch",
	ja: "Japanese",
	zh: "Chinese",
	ko: "Korean",
	ru: "Russian",
};

/** Store state */
interface DictationStoreState {
	enabled: boolean;
	hotkey: string;
	language: string;
	selectedModel: string;
	selectedDevice: string | null;
	models: ModelInfo[];
	modelStatus: ModelStatus;
	modelName: string;
	modelSizeMb: number;
	recording: boolean;
	processing: boolean;
	loading: boolean; // Model is being loaded into memory on first use
	downloading: boolean;
	downloadPercent: number;
	corrections: Record<string, string>;
	devices: AudioDevice[];
	longPressMs: number;
	autoSend: boolean;
	/**
	 * Whether arming and disarming hands-free tell the bound model so.
	 *
	 * Defaults to true, mirroring the Rust default: the voice tool is offered
	 * whether or not this is set, and a model with no reason to speak answers
	 * in text.
	 */
	notifyModelOnHandsFree: boolean;
	/** Hold-back between a hands-free transcript and its enqueue, in ms. */
	handsFreeHoldBackMs: number;
	/** Phrase that must open each hands-free turn; empty means ungated. */
	handsFreeActivationPhrase: string;
	/** Which voice speaks. Empty means the language's first, decided in Rust. */
	speechVoice: string;
	/** The speech catalogue and what state each entry is in. */
	speechAssets: SpeechAsset[];
	/** Download percent per asset id, present only while one is downloading. */
	speechDownloads: Record<string, number | undefined>;
	/**
	 * Live hands-free state, or null before anything has polled for it.
	 *
	 * Null is not "disarmed": nothing here ever arms by itself, and the
	 * difference between "not asked yet" and "asked, and nothing is armed"
	 * decides whether the panel may show a phase at all.
	 */
	handsFree: HandsFreeStatus | null;
	/** What the last arm or disarm refused to do, cleared by the next one. */
	handsFreeError: string | null;
	/** Live speaker state, or null before anything has polled for it. */
	speech: SpeechStatus | null;
	rmsThreshold: number;
	noSpeechThreshold: number;
	capturingHotkey: boolean;
	partialText: string;
	/** Normalized live microphone level used by the dictation preview meter. */
	audioLevel: number;
	backendInfo: DictationBackend | null;
	/**
	 * Why the last recording produced no text, or null when it produced some.
	 *
	 * Tuning the two thresholds is guesswork without it: a gate that rejects
	 * speech and a microphone that captured nothing look identical from the
	 * outside. Settings > Dictation renders this verbatim.
	 */
	lastSkipReason: string | null;
}

function createDictationStore() {
	const [state, setState] = createStore<DictationStoreState>({
		enabled: false,
		hotkey: "F5",
		language: "auto",
		selectedModel: "large-v3-turbo",
		selectedDevice: null,
		models: [],
		modelStatus: "not_downloaded",
		modelName: "",
		modelSizeMb: 0,
		recording: false,
		processing: false,
		loading: false,
		downloading: false,
		downloadPercent: 0,
		corrections: {},
		devices: [],
		longPressMs: 400,
		autoSend: false,
		notifyModelOnHandsFree: true,
		handsFreeHoldBackMs: DEFAULT_HOLD_BACK_MS,
		handsFreeActivationPhrase: "",
		speechVoice: "",
		speechAssets: [],
		speechDownloads: {},
		handsFree: null,
		handsFreeError: null,
		speech: null,
		rmsThreshold: DEFAULT_RMS_THRESHOLD,
		noSpeechThreshold: DEFAULT_NO_SPEECH_THRESHOLD,
		capturingHotkey: false,
		partialText: "",
		audioLevel: 0,
		backendInfo: null,
		lastSkipReason: null,
	});

	// Listen for download progress events from Rust
	listen<DownloadProgress>("dictation-download-progress", (event) => {
		setState("downloadPercent", event.payload.percent);
	});

	// Listen for streaming partial transcription results
	listen<string>("dictation-partial", (event) => {
		setState("partialText", event.payload);
	});

	// Per-asset speech download progress. Keyed by asset id because the runtime
	// library and a language are separate downloads a user can start together,
	// and one shared percent would show each of them the other's.
	listen<{ asset: string; percent: number }>("speech-download-progress", (event) => {
		setState("speechDownloads", event.payload.asset, event.payload.percent);
	});

	let audioLevelTimer: ReturnType<typeof setInterval> | null = null;
	const stopAudioLevelPolling = () => {
		if (audioLevelTimer) clearInterval(audioLevelTimer);
		audioLevelTimer = null;
	};
	const startAudioLevelPolling = () => {
		stopAudioLevelPolling();
		audioLevelTimer = setInterval(() => {
			void invoke<DictationStatus>("get_dictation_status")
				.then((status) => setState("audioLevel", normalizeAudioLevel(status.audio_level)))
				.catch(() => stopAudioLevelPolling());
		}, 75);
	};

	// Listen for backend info (gpu/cpu) after model load
	listen<{ backend: DictationBackend }>("dictation-backend-info", (event) => {
		setState("backendInfo", event.payload.backend);
	});

	const actions = {
		/** Load config from Rust backend (file-based) */
		async refreshConfig(): Promise<void> {
			if (!isTauri()) return;
			try {
				const config = await invoke<DictationConfig>("get_dictation_config");
				setState({
					enabled: config.enabled,
					hotkey: config.hotkey,
					language: config.language,
					selectedModel: config.model ?? "large-v3-turbo",
					selectedDevice: config.device ?? null,
					longPressMs: config.long_press_ms ?? 400,
					autoSend: config.auto_send ?? false,
					notifyModelOnHandsFree: config.hands_free_notify_model ?? true,
					handsFreeHoldBackMs: config.hands_free_hold_back_ms ?? DEFAULT_HOLD_BACK_MS,
					handsFreeActivationPhrase: config.hands_free_activation_phrase ?? "",
					speechVoice: config.speech_voice ?? "",
					rmsThreshold: config.rms_threshold ?? DEFAULT_RMS_THRESHOLD,
					noSpeechThreshold: config.no_speech_threshold ?? DEFAULT_NO_SPEECH_THRESHOLD,
				});
			} catch (err) {
				appLogger.error("dictation", "Failed to get dictation config", err);
			}
		},

		/**
		 * Save a single config field to disk via Rust.
		 *
		 * Load-modify-save, not build-from-scratch. `set_dictation_config`
		 * writes the whole document and every hands-free field carries
		 * `#[serde(default)]` so an older config still loads — which means a
		 * payload that omits a field silently resets it instead of failing.
		 * Rebuilding this object from store state therefore erased every
		 * setting the UI has no control for. Spread the stored config first and
		 * override only the fields this surface owns.
		 */
		async saveConfig(partial: Partial<DictationConfig>): Promise<void> {
			try {
				// A save that cannot read first is abandoned: writing a config
				// assembled from defaults is how the fields below got lost.
				// "Could not read" includes an answer that is not a config —
				// the fields below are read off it by name, and a save built on
				// nothing is the very thing this guard exists to stop.
				const stored = await invoke<DictationConfig>("get_dictation_config");
				if (!stored || typeof stored !== "object") {
					appLogger.error("dictation", "Refusing to save: the stored config could not be read");
					return;
				}
				const config: DictationConfig = {
					...stored,
					enabled: partial.enabled ?? state.enabled,
					hotkey: partial.hotkey ?? state.hotkey,
					language: partial.language ?? state.language,
					model: partial.model ?? state.selectedModel,
					device: partial.device !== undefined ? partial.device : state.selectedDevice,
					long_press_ms: partial.long_press_ms ?? state.longPressMs,
					auto_send: partial.auto_send ?? state.autoSend,
					hands_free_notify_model: partial.hands_free_notify_model ?? state.notifyModelOnHandsFree,
					// These three fall back to the *stored* value, not to store
					// state. Their controls live in one panel, so a save from
					// anywhere else runs with store state that was never loaded
					// from disk — and the fallback would then write this
					// session's default over a setting the user had chosen. The
					// fields above are kept in sync by every surface that owns
					// them, which is why they may read from state.
					hands_free_hold_back_ms: partial.hands_free_hold_back_ms ?? stored.hands_free_hold_back_ms,
					hands_free_activation_phrase: partial.hands_free_activation_phrase ?? stored.hands_free_activation_phrase,
					speech_voice: partial.speech_voice ?? stored.speech_voice,
					rms_threshold: partial.rms_threshold ?? state.rmsThreshold,
					no_speech_threshold: partial.no_speech_threshold ?? state.noSpeechThreshold,
				};
				await invoke("set_dictation_config", { config });
				// Map DictationConfig fields to DictationStoreState fields
				const storeUpdate: Partial<DictationStoreState> = {};
				if (partial.enabled !== undefined) storeUpdate.enabled = partial.enabled;
				if (partial.hotkey !== undefined) storeUpdate.hotkey = partial.hotkey;
				if (partial.language !== undefined) storeUpdate.language = partial.language;
				if (partial.model !== undefined) storeUpdate.selectedModel = partial.model;
				if (partial.device !== undefined) storeUpdate.selectedDevice = partial.device;
				if (partial.long_press_ms !== undefined) storeUpdate.longPressMs = partial.long_press_ms;
				if (partial.auto_send !== undefined) storeUpdate.autoSend = partial.auto_send;
				if (partial.hands_free_notify_model !== undefined)
					storeUpdate.notifyModelOnHandsFree = partial.hands_free_notify_model;
				if (partial.hands_free_hold_back_ms !== undefined)
					storeUpdate.handsFreeHoldBackMs = partial.hands_free_hold_back_ms;
				if (partial.hands_free_activation_phrase !== undefined)
					storeUpdate.handsFreeActivationPhrase = partial.hands_free_activation_phrase;
				if (partial.speech_voice !== undefined) storeUpdate.speechVoice = partial.speech_voice;
				if (partial.rms_threshold !== undefined) storeUpdate.rmsThreshold = partial.rms_threshold;
				if (partial.no_speech_threshold !== undefined) storeUpdate.noSpeechThreshold = partial.no_speech_threshold;
				setState(storeUpdate);
			} catch (err) {
				appLogger.error("dictation", "Failed to save dictation config", err);
			}
		},

		setEnabled(value: boolean): void {
			actions.saveConfig({ enabled: value });
		},

		setHotkey(value: string): void {
			actions.saveConfig({ hotkey: value });
		},

		setCapturingHotkey(value: boolean): void {
			setState("capturingHotkey", value);
		},

		setLongPressMs(value: number): void {
			actions.saveConfig({ long_press_ms: value });
		},

		setNotifyModelOnHandsFree(value: boolean): void {
			actions.saveConfig({ hands_free_notify_model: value });
		},

		setHandsFreeHoldBackMs(value: number): void {
			actions.saveConfig({ hands_free_hold_back_ms: value });
		},

		/**
		 * Set the phrase that must open each hands-free turn.
		 *
		 * Trimmed here because the matching is Rust's and it compares words:
		 * a phrase saved with a trailing space would be a phrase no utterance
		 * ever opens with, and nothing on screen would say why.
		 */
		setHandsFreeActivationPhrase(value: string): void {
			actions.saveConfig({ hands_free_activation_phrase: value.trim() });
		},

		setSpeechVoice(value: string): void {
			actions.saveConfig({ speech_voice: value });
		},

		setAutoSend(value: boolean): void {
			actions.saveConfig({ auto_send: value });
		},

		setRmsThreshold(value: number): void {
			actions.saveConfig({ rms_threshold: value });
		},

		setNoSpeechThreshold(value: number): void {
			actions.saveConfig({ no_speech_threshold: value });
		},

		setLanguage(value: string): void {
			actions.saveConfig({ language: value });
		},

		setDevice(value: string | null): void {
			actions.saveConfig({ device: value });
		},

		/** Refresh status from Rust backend */
		async refreshStatus(): Promise<void> {
			try {
				const status = await invoke<DictationStatus>("get_dictation_status");
				setState({
					modelStatus: status.model_status,
					modelName: status.model_name,
					modelSizeMb: status.model_size_mb,
					recording: status.recording,
					processing: status.processing,
					audioLevel: normalizeAudioLevel(status.audio_level),
				});
			} catch (err) {
				appLogger.error("dictation", "Failed to get dictation status", err);
			}
		},

		/** Refresh correction map from Rust backend */
		async refreshCorrections(): Promise<void> {
			try {
				const map = await invoke<Record<string, string>>("get_correction_map");
				setState("corrections", map);
			} catch (err) {
				appLogger.error("dictation", "Failed to get correction map", err);
			}
		},

		/** Save correction map to Rust backend */
		async saveCorrections(map: Record<string, string>): Promise<void> {
			try {
				await invoke("set_correction_map", { map });
				setState("corrections", map);
			} catch (err) {
				appLogger.error("dictation", "Failed to save corrections", err);
			}
		},

		/** List available audio devices */
		async refreshDevices(): Promise<void> {
			try {
				const devices = await invoke<AudioDevice[]>("list_audio_devices");
				setState("devices", devices);
			} catch (err) {
				appLogger.error("dictation", "Failed to list audio devices", err);
			}
		},

		/** Fetch available model info from Rust backend */
		async refreshModels(): Promise<void> {
			try {
				const models = await invoke<ModelInfo[]>("get_model_info");
				setState("models", models);
			} catch (err) {
				appLogger.error("dictation", "Failed to get model info", err);
			}
		},

		/** Set the selected model and persist to config */
		async setModel(name: string): Promise<void> {
			await actions.saveConfig({ model: name });
			setState("selectedModel", name);
		},

		/** Delete a downloaded model and refresh the model list */
		async deleteModel(name: string): Promise<void> {
			try {
				await invoke("delete_whisper_model", { modelName: name });
				await actions.refreshModels();
			} catch (err) {
				appLogger.error("dictation", "Failed to delete model", err);
			}
		},

		/** Download a Whisper model (defaults to selectedModel) */
		async downloadModel(modelName?: string): Promise<void> {
			setState("downloading", true);
			setState("downloadPercent", 0);
			try {
				await invoke<string>("download_whisper_model", { modelName: modelName ?? state.selectedModel });
				await actions.refreshStatus();
				await actions.refreshModels();
			} catch (err) {
				appLogger.error("dictation", "Model download failed", err);
			} finally {
				setState("downloading", false);
			}
		},

		/** Start recording (sets loading=true while model initializes on first use) */
		async startRecording(): Promise<void> {
			setState("loading", true);
			try {
				await invoke("start_dictation");
				setState("recording", true);
				setState("audioLevel", 0);
				startAudioLevelPolling();
			} catch (err) {
				const errStr = String(err);
				if (errStr.includes("microphone_denied")) {
					appLogger.error(
						"dictation",
						"Microphone access denied. Open System Settings > Privacy > Microphone to allow access.",
					);
					invoke("open_microphone_settings").catch(() => {});
				} else if (errStr.includes("microphone_restricted")) {
					appLogger.error("dictation", "Microphone access restricted by system policy");
				} else {
					appLogger.error("dictation", "Failed to start recording", err);
				}
				throw err;
			} finally {
				setState("loading", false);
			}
		},

		/** Stop recording and get transcription result */
		async stopRecording(): Promise<TranscribeResponse | null> {
			// Guard against concurrent stop calls — the Rust side rejects "Not recording"
			// but we avoid the noise by checking frontend state first.
			if (!state.recording) return null;
			// Optimistically clear recording so concurrent callers bail out above.
			setState("recording", false);
			setState("audioLevel", 0);
			stopAudioLevelPolling();
			try {
				const response = await invoke<TranscribeResponse>("stop_dictation_and_transcribe");
				setState("processing", false);
				setState("partialText", "");
				setState("audioLevel", 0);
				setState("lastSkipReason", response.skip_reason);
				return response;
			} catch (err) {
				appLogger.error("dictation", "Failed to stop recording", err);
				setState("processing", false);
				setState("partialText", "");
				setState("audioLevel", 0);
				setState("lastSkipReason", "transcription failed");
				return null;
			}
		},

		// --- Speech assets (818-2a29) ---------------------------------------

		/** Load the speech catalogue and what state each entry is in. */
		async refreshSpeechAssets(): Promise<void> {
			try {
				setState("speechAssets", await invoke<SpeechAsset[]>("get_speech_assets"));
			} catch (err) {
				appLogger.error("dictation", "Failed to list speech assets", err);
			}
		},

		/**
		 * Download one asset, then re-read the catalogue.
		 *
		 * The percent comes from the `speech-download-progress` event rather
		 * than from here; this only marks the asset as started so the bar
		 * appears before the first event, and clears it either way — a failed
		 * download that left its last percent behind would read as one still
		 * running.
		 */
		async downloadSpeechAsset(id: string): Promise<void> {
			setState("speechDownloads", id, 0);
			try {
				await invoke<string>("download_speech_asset", { asset: id });
			} catch (err) {
				appLogger.error("dictation", `Speech asset download failed: ${id}`, err);
			} finally {
				// By key, not by returning a smaller object: a store update at
				// a path merges, so a rest-spread that drops the key leaves it
				// exactly where it was.
				setState("speechDownloads", id, undefined);
				await actions.refreshSpeechAssets();
			}
		},

		/**
		 * Ask Rust to stop a download.
		 *
		 * The bar is left alone: the download is still running until the
		 * in-flight `downloadSpeechAsset` returns, and clearing it here would
		 * show a finished download that is still writing to disk.
		 */
		async cancelSpeechDownload(id: string): Promise<void> {
			try {
				await invoke<string>("cancel_speech_download", { asset: id });
			} catch (err) {
				appLogger.error("dictation", `Failed to cancel download: ${id}`, err);
			}
		},

		async deleteSpeechAsset(id: string): Promise<void> {
			try {
				await invoke<string>("delete_speech_asset", { asset: id });
			} catch (err) {
				appLogger.error("dictation", `Failed to delete speech asset: ${id}`, err);
			}
			await actions.refreshSpeechAssets();
		},

		// --- Hands-free conversation (818-2a29) -----------------------------

		/**
		 * Read the live hands-free state.
		 *
		 * Deliberately one call. The hotkey asks this on every press to find
		 * out whether it is starting a recording or ending a conversation, and
		 * the speaker state below is not part of that answer.
		 */
		async refreshHandsFree(): Promise<void> {
			try {
				setState("handsFree", await invoke<HandsFreeStatus>("get_hands_free_status"));
			} catch (err) {
				appLogger.error("dictation", "Failed to get hands-free status", err);
			}
		},

		/** Read what the speaker is doing: queued, synthesising, playing. */
		async refreshSpeechStatus(): Promise<void> {
			try {
				setState("speech", await invoke<SpeechStatus>("get_speech_status"));
			} catch (err) {
				appLogger.error("dictation", "Failed to get speech status", err);
			}
		},

		/**
		 * Bind hands-free to a terminal and open the microphone.
		 *
		 * Only ever from a user action — nothing here runs on mount, so a
		 * restart never re-opens the microphone by itself.
		 *
		 * The owner is always the desktop endpoint: Rust refuses any other one
		 * rather than opening the microphone on the machine running
		 * TUICommander, so a browser tab gets the refusal verbatim instead of
		 * silently arming somebody else's hardware.
		 */
		async armHandsFree(sessionId: string): Promise<boolean> {
			setState("handsFreeError", null);
			try {
				setState(
					"handsFree",
					await invoke<HandsFreeStatus>("arm_hands_free_dictation", {
						sessionId,
						owner: DESKTOP_AUDIO_OWNER,
					}),
				);
				await actions.refreshSpeechStatus();
				return true;
			} catch (err) {
				setState("handsFreeError", String(err));
				appLogger.error("dictation", "Failed to arm hands-free", err);
				return false;
			}
		},

		/**
		 * Stop the conversation: the microphone, the pending transcript and
		 * anything queued or being spoken.
		 *
		 * Reports what it could not take back. Voice entries the composer
		 * already typed cannot be retracted, and saying so is the difference
		 * between an honest outcome and a claim.
		 */
		async disarmHandsFree(): Promise<number[]> {
			setState("handsFreeError", null);
			try {
				const result = await invoke<{
					alreadyDelivered: number[];
					status: HandsFreeStatus;
				}>("disarm_hands_free_dictation");
				setState("handsFree", result.status);
				await actions.refreshSpeechStatus();
				return result.alreadyDelivered;
			} catch (err) {
				setState("handsFreeError", String(err));
				appLogger.error("dictation", "Failed to disarm hands-free", err);
				return [];
			}
		},

		/** Inject text (apply corrections) without recording */
		async injectText(text: string): Promise<string | null> {
			try {
				return await invoke<string>("inject_text", { text });
			} catch (err) {
				appLogger.error("dictation", "Failed to inject text", err);
				return null;
			}
		},
	};

	return { state, ...actions };
}

export const dictationStore = createDictationStore();
