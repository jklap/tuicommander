import { type Accessor, createEffect, createSignal, onCleanup } from "solid-js";
import { t } from "../i18n";
import { appLogger } from "../stores/appLogger";
import { toastsStore } from "../stores/toasts";
import { rpc } from "../transport";

interface RecognitionResult {
	isFinal: boolean;
	readonly [index: number]: { transcript: string };
}

interface BrowserRecognition {
	lang: string;
	continuous: boolean;
	interimResults: boolean;
	onresult: ((event: { results: ArrayLike<RecognitionResult> }) => void) | null;
	onerror: ((event: { error: string }) => void) | null;
	onend: (() => void) | null;
	start(): void;
	stop(): void;
	abort(): void;
}

type RecognitionConstructor = new () => BrowserRecognition;

/** Browser input interaction only; server hands-free remains a separate conversation. */
export function useBrowserDictation(options: {
	blocked: Accessor<boolean>;
	getDraft: Accessor<string>;
	setDraft: (text: string) => void;
	onRecording?: (recording: boolean) => void;
}) {
	const browser = window as Window & {
		SpeechRecognition?: RecognitionConstructor;
		webkitSpeechRecognition?: RecognitionConstructor;
	};
	const Constructor = browser.SpeechRecognition ?? browser.webkitSpeechRecognition;
	const [recording, setRecording] = createSignal(false);
	let recognition: BrowserRecognition | null = null;
	let language = navigator.language;
	let renderedSpeech = "";

	if (Constructor) {
		void rpc<{ language?: string }>("get_dictation_config")
			.then((config) => {
				if (config?.language && config.language !== "auto") language = config.language;
			})
			.catch((error) => appLogger.info("dictation", "Browser dictation uses browser language", error));
	}

	function markRecording(active: boolean) {
		setRecording(active);
		options.onRecording?.(active);
	}

	function fail(error: unknown) {
		toastsStore.add(t("mobile.dictation.error", "Dictation failed"), String(error), "error", true);
	}

	function cancel() {
		const current = recognition;
		recognition = null;
		if (current) {
			current.onresult = current.onerror = current.onend = null;
			current.abort();
		}
		markRecording(false);
	}

	createEffect(() => {
		if (options.blocked() && recognition) cancel();
	});
	onCleanup(cancel);

	function toggle() {
		if (recognition) {
			try {
				recognition.stop();
			} catch (error) {
				cancel();
				fail(error);
			}
			return;
		}
		if (!Constructor || options.blocked()) return;
		try {
			const current = new Constructor();
			recognition = current;
			renderedSpeech = "";
			current.lang = language;
			current.continuous = true;
			current.interimResults = true;
			current.onresult = (event) => {
				if (recognition !== current) return;
				const draft = options.getDraft();
				// Replace the previous hypothesis only while it still owns the draft tail.
				// A user edit starts a new append boundary rather than losing typed text.
				const base = renderedSpeech && draft.endsWith(renderedSpeech) ? draft.slice(0, -renderedSpeech.length) : draft;
				const speech = Array.from(event.results, (result) => result[0].transcript.trim())
					.join(" ")
					.trim();
				renderedSpeech = speech ? `${base && !/\s$/.test(base) ? " " : ""}${speech}` : "";
				options.setDraft(base + renderedSpeech);
			};
			current.onerror = (event) => {
				cancel();
				fail(event.error);
			};
			current.onend = () => {
				if (recognition !== current) return;
				recognition = null;
				markRecording(false);
			};
			// Claim the microphone before start(), including the permission prompt.
			markRecording(true);
			current.start();
		} catch (error) {
			cancel();
			fail(error);
		}
	}

	return { available: !!Constructor, recording, toggle, cancel };
}
