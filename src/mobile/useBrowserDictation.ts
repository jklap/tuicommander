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
	let discarding = false;
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
		appLogger.warn("dictation", "Browser dictation failed", error);
		toastsStore.add(t("mobile.dictation.error", "Dictation failed"), String(error), "error", true);
	}

	function release() {
		if (recognition) recognition.onresult = recognition.onerror = recognition.onend = null;
		recognition = null;
		markRecording(false);
	}

	function cancel() {
		if (!recognition || discarding) return;
		discarding = true;
		// abort() is asynchronous: keep the service and microphone claim until end.
		try {
			recognition.abort();
		} catch (error) {
			release();
			fail(error);
		}
	}

	createEffect(() => {
		if (options.blocked() && recognition) cancel();
	});
	onCleanup(cancel);

	function toggle() {
		if (recognition) {
			if (discarding) return;
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
			discarding = false;
			renderedSpeech = "";
			current.lang = language;
			current.continuous = true;
			current.interimResults = true;
			current.onresult = (event) => {
				if (recognition !== current || discarding) return;
				const draft = options.getDraft();
				// Replace the previous hypothesis only while it still owns the draft tail.
				// Manual composer edits cancel recognition before changing the draft.
				const base = renderedSpeech && draft.endsWith(renderedSpeech) ? draft.slice(0, -renderedSpeech.length) : draft;
				const speech = Array.from(event.results, (result) => result[0].transcript.trim())
					.join(" ")
					.trim();
				renderedSpeech = speech ? `${base && !/\s$/.test(base) ? " " : ""}${speech}` : "";
				options.setDraft(base + renderedSpeech);
			};
			current.onerror = (event) => {
				if (recognition !== current || discarding) return;
				discarding = true;
				fail(event.error);
			};
			current.onend = () => {
				if (recognition !== current) return;
				release();
			};
			// Claim the microphone before start(), including the permission prompt.
			markRecording(true);
			current.start();
		} catch (error) {
			// A synchronous start failure has no running service to emit end.
			release();
			fail(error);
		}
	}

	return { available: !!Constructor, recording, toggle, cancel };
}
