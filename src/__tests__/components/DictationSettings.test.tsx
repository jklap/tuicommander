import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";

const mockStore = vi.hoisted(() => ({
	state: {
		enabled: false,
		hotkey: "F5",
		language: "auto",
		selectedModel: "large-v3-turbo",
		selectedDevice: null as string | null,
		models: [
			{ name: "small", display_name: "Small", size_hint_mb: 488, downloaded: false, actual_size_mb: 0 },
			{
				name: "large-v3-turbo",
				display_name: "Large V3 Turbo",
				size_hint_mb: 1620,
				downloaded: true,
				actual_size_mb: 1620,
			},
		],
		modelStatus: "downloaded",
		modelName: "large-v3-turbo",
		modelSizeMb: 1620,
		recording: false,
		processing: false,
		downloading: false,
		downloadPercent: 0,
		corrections: {},
		devices: [] as { name: string; is_default: boolean }[],
		longPressMs: 400,
		autoSend: true,
		rmsThreshold: 0.001,
		noSpeechThreshold: 0.6,
		audioLevel: 0,
		partialText: "",
		lastSkipReason: null as string | null,
		// The spoken-reply and hands-free half of the store. The panel reads
		// these unconditionally — `languageAsset()` calls `.find` on
		// `speechAssets` while deciding what to render — so leaving them out
		// does not merely skip those sections, it throws before the model list
		// this file is about is ever drawn.
		notifyModelOnHandsFree: true,
		handsFreeHoldBackMs: 1500,
		handsFreeEarcons: true,
		handsFreeStartNotice: "",
		handsFreeActivationPhrase: "",
		speechVoice: "",
		speechVolumeDb: -18,
		speechLevelling: 0.67,
		speechAssets: [] as unknown[],
		speechVoices: [] as { id: string; source: string }[],
		speechDownloads: {} as Record<string, number | undefined>,
		handsFree: null as unknown,
		handsFreeError: null as string | null,
		speech: null as unknown,
	},
	refreshConfig: vi.fn(),
	refreshStatus: vi.fn(),
	refreshCorrections: vi.fn(),
	refreshModels: vi.fn(),
	setEnabled: vi.fn(),
	setHotkey: vi.fn(),
	setLanguage: vi.fn(),
	setDevice: vi.fn(),
	setModel: vi.fn(),
	deleteModel: vi.fn(),
	downloadModel: vi.fn(),
	saveConfig: vi.fn(),
	saveCorrections: vi.fn(),
	refreshDevices: vi.fn(),
	startRecording: vi.fn(),
	stopRecording: vi.fn(),
	injectText: vi.fn(),
	setCapturingHotkey: vi.fn(),
	setLongPressMs: vi.fn(),
	setAutoSend: vi.fn(),
	setRmsThreshold: vi.fn(),
	setNoSpeechThreshold: vi.fn(),
	refreshSpeechAssets: vi.fn(),
	refreshSpeechStatus: vi.fn(),
	downloadSpeechAsset: vi.fn(),
	cancelSpeechDownload: vi.fn(),
	deleteSpeechAsset: vi.fn(),
	setSpeechVoice: vi.fn(),
	setSpeechVolumeDb: vi.fn(),
	setSpeechLevelling: vi.fn(),
	refreshSpeechVoices: vi.fn(),
	importSpeechVoice: vi.fn(() => Promise.resolve(null as string | null)),
	deleteSpeechVoice: vi.fn(),
	previewSpeechVoice: vi.fn(() => Promise.resolve(null as string | null)),
	refreshHandsFree: vi.fn(),
	armHandsFree: vi.fn(),
	disarmHandsFree: vi.fn(),
	setHandsFreeHoldBackMs: vi.fn(),
	setHandsFreeEarcons: vi.fn(),
	setHandsFreeActivationPhrase: vi.fn(),
	setNotifyModelOnHandsFree: vi.fn(),
	setHandsFreeStartNotice: vi.fn(),
	resetHandsFreeStartNotice: vi.fn(),
	getDefaultHandsFreeStartNotice: vi.fn(() => Promise.resolve("")),
}));

vi.mock("../../stores/ui", async () => {
	const { createStore } = await import("solid-js/store");
	const [state, setState] = createStore({ settingsExpertMode: false });
	return {
		uiStore: {
			state,
			setSettingsExpertMode: (enabled: boolean) => setState("settingsExpertMode", enabled),
		},
	};
});

vi.mock("../../stores/dictation", () => ({
	dictationStore: mockStore,
	WHISPER_LANGUAGES: { auto: "Auto-detect", en: "English", it: "Italian", ja: "Japanese" },
}));

import { DictationSettings } from "../../components/SettingsPanel/DictationSettings";
import { settingsExpertStore } from "../../stores/settingsExpert";
import { uiStore } from "../../stores/ui";

describe("DictationSettings – Model Selector", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		// Default: check_microphone_permission returns "not_determined" (no auto-detect)
		mockInvoke.mockResolvedValue("not_determined");
		// Reset models to default test data
		mockStore.state.models = [
			{ name: "small", display_name: "Small", size_hint_mb: 488, downloaded: false, actual_size_mb: 0 },
			{
				name: "large-v3-turbo",
				display_name: "Large V3 Turbo",
				size_hint_mb: 1620,
				downloaded: true,
				actual_size_mb: 1620,
			},
		];
		mockStore.state.selectedModel = "large-v3-turbo";
		mockStore.state.selectedDevice = null;
		mockStore.state.downloading = false;
		mockStore.state.downloadPercent = 0;
		mockStore.state.devices = [];
	});

	it("calls refreshModels on mount", () => {
		render(() => <DictationSettings />);
		expect(mockStore.refreshModels).toHaveBeenCalledOnce();
	});

	it("renders a row for each model", () => {
		const { container } = render(() => <DictationSettings />);
		const rows = container.querySelectorAll(".modelRow");
		expect(rows.length).toBe(2);
	});

	it("shows display name and size hint for each model", () => {
		const { container } = render(() => <DictationSettings />);
		const rows = container.querySelectorAll(".modelRow");

		expect(rows[0].textContent).toContain("Small");
		expect(rows[0].textContent).toContain("488");
		expect(rows[1].textContent).toContain("Large V3 Turbo");
		expect(rows[1].textContent).toContain("1620");
	});

	it("shows download button for not-downloaded models", () => {
		const { container } = render(() => <DictationSettings />);
		const rows = container.querySelectorAll(".modelRow");
		const downloadBtn = rows[0].querySelector(".modelDownload");
		expect(downloadBtn).not.toBeNull();
		expect(downloadBtn!.textContent).toContain("Download");
	});

	it("shows delete button for downloaded models", () => {
		const { container } = render(() => <DictationSettings />);
		const rows = container.querySelectorAll(".modelRow");
		const deleteBtn = rows[1].querySelector(".modelDelete");
		expect(deleteBtn).not.toBeNull();
	});

	it("marks the selected model as active", () => {
		const { container } = render(() => <DictationSettings />);
		const rows = container.querySelectorAll(".modelRow");
		expect(rows[1].classList.contains("active")).toBe(true);
		expect(rows[0].classList.contains("active")).toBe(false);
	});

	it("clicking a downloaded model calls setModel", () => {
		// Both models downloaded for this test
		mockStore.state.models = [
			{ name: "small", display_name: "Small", size_hint_mb: 488, downloaded: true, actual_size_mb: 488 },
			{
				name: "large-v3-turbo",
				display_name: "Large V3 Turbo",
				size_hint_mb: 1620,
				downloaded: true,
				actual_size_mb: 1620,
			},
		];

		const { container } = render(() => <DictationSettings />);
		const rows = container.querySelectorAll(".modelRow");
		const selectBtn = rows[0].querySelector(".modelSelect");
		expect(selectBtn).not.toBeNull();
		fireEvent.click(selectBtn!);
		expect(mockStore.setModel).toHaveBeenCalledWith("small");
	});

	it("download button calls downloadModel with model name", () => {
		const { container } = render(() => <DictationSettings />);
		const rows = container.querySelectorAll(".modelRow");
		const downloadBtn = rows[0].querySelector(".modelDownload");
		fireEvent.click(downloadBtn!);
		expect(mockStore.downloadModel).toHaveBeenCalledWith("small");
	});

	it("delete button calls deleteModel with model name", () => {
		const { container } = render(() => <DictationSettings />);
		const rows = container.querySelectorAll(".modelRow");
		const deleteBtn = rows[1].querySelector(".modelDelete");
		fireEvent.click(deleteBtn!);
		expect(mockStore.deleteModel).toHaveBeenCalledWith("large-v3-turbo");
	});

	it("does not allow selecting a not-downloaded model", () => {
		const { container } = render(() => <DictationSettings />);
		const rows = container.querySelectorAll(".modelRow");
		// Not-downloaded model should not have a select button
		const selectBtn = rows[0].querySelector(".modelSelect");
		expect(selectBtn).toBeNull();
	});

	it("shows status badge for each model", () => {
		const { container } = render(() => <DictationSettings />);
		const rows = container.querySelectorAll(".modelRow");

		const badge0 = rows[0].querySelector(".modelBadge");
		expect(badge0).not.toBeNull();

		const badge1 = rows[1].querySelector(".modelBadge");
		expect(badge1).not.toBeNull();
		expect(badge1!.textContent).toContain("Downloaded");
	});

	it("shows progress bar when downloading", () => {
		mockStore.state.downloading = true;
		mockStore.state.downloadPercent = 42;
		// Mark which model is being downloaded by setting selectedModel to the downloading one
		mockStore.state.selectedModel = "small";

		const { container } = render(() => <DictationSettings />);
		const progressBar = container.querySelector(".progressFill");
		expect(progressBar).not.toBeNull();
	});
});

describe("DictationSettings – Microphone Selector", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockResolvedValue("not_determined");
		mockStore.state.devices = [];
		mockStore.state.selectedDevice = null;
	});

	it("shows Detect Microphones button when no devices loaded", () => {
		const { container } = render(() => <DictationSettings />);
		const buttons = Array.from(container.querySelectorAll("button"));
		const detectBtn = buttons.find((b) => b.textContent?.includes("Detect"));
		expect(detectBtn).not.toBeNull();
	});

	/** Find the device <select> by looking for one whose first option says "System Default" */
	function findDeviceSelect(container: HTMLElement): HTMLSelectElement | null {
		const selects = container.querySelectorAll("select");
		for (const s of selects) {
			if (s.querySelector("option")?.textContent?.includes("System Default")) return s;
		}
		return null;
	}

	it("shows device dropdown with System Default when devices are loaded", () => {
		mockStore.state.devices = [
			{ name: "Built-in Microphone", is_default: true },
			{ name: "USB Mic", is_default: false },
		];

		const { container } = render(() => <DictationSettings />);
		const select = findDeviceSelect(container);
		expect(select).not.toBeNull();
		const options = select!.querySelectorAll("option");
		// System Default + 2 devices = 3 options
		expect(options.length).toBe(3);
		expect(options[0].textContent).toContain("System Default");
		expect(options[0].value).toBe("");
		expect(options[1].textContent).toContain("Built-in Microphone");
		expect(options[2].textContent).toContain("USB Mic");
	});

	it("calls setDevice when selecting a specific device", () => {
		mockStore.state.devices = [
			{ name: "Built-in Microphone", is_default: true },
			{ name: "USB Mic", is_default: false },
		];

		const { container } = render(() => <DictationSettings />);
		const select = findDeviceSelect(container)!;
		fireEvent.change(select, { target: { value: "USB Mic" } });
		expect(mockStore.setDevice).toHaveBeenCalledWith("USB Mic");
	});

	it("calls setDevice(null) when selecting System Default", () => {
		mockStore.state.devices = [{ name: "Built-in Microphone", is_default: true }];
		mockStore.state.selectedDevice = "Built-in Microphone";

		const { container } = render(() => <DictationSettings />);
		const select = findDeviceSelect(container)!;
		fireEvent.change(select, { target: { value: "" } });
		expect(mockStore.setDevice).toHaveBeenCalledWith(null);
	});

	it("auto-detects devices on mount when mic is authorized", async () => {
		mockInvoke.mockResolvedValue("authorized");

		render(() => <DictationSettings />);
		// Wait for the async onMount to complete
		await new Promise((r) => setTimeout(r, 0));

		expect(mockInvoke).toHaveBeenCalledWith("check_microphone_permission");
		expect(mockStore.refreshDevices).toHaveBeenCalled();
	});

	it("does not auto-detect devices when mic is not determined", async () => {
		mockInvoke.mockResolvedValue("not_determined");

		render(() => <DictationSettings />);
		await new Promise((r) => setTimeout(r, 0));

		expect(mockInvoke).toHaveBeenCalledWith("check_microphone_permission");
		expect(mockStore.refreshDevices).not.toHaveBeenCalled();
	});

	it("logs warning when mic is denied", async () => {
		mockInvoke.mockResolvedValue("denied");
		const consoleSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

		render(() => <DictationSettings />);
		await new Promise((r) => setTimeout(r, 0));

		const warnCalls = consoleSpy.mock.calls;
		const micWarning = warnCalls.find(
			(call) => typeof call[1] === "string" && call[1].includes("Microphone access denied"),
		);
		expect(micWarning).toBeDefined();
		expect(mockStore.refreshDevices).not.toHaveBeenCalled();
		consoleSpy.mockRestore();
	});
});

describe("DictationSettings – Voice Tuning", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockResolvedValue("not_determined");
		mockStore.state.rmsThreshold = 0.001;
		mockStore.state.noSpeechThreshold = 0.6;
		mockStore.state.audioLevel = 0;
		mockStore.state.partialText = "";
		mockStore.state.lastSkipReason = null;
		mockStore.state.recording = false;
		mockStore.state.processing = false;
	});

	/** The level gate slider and the marker both read in meter units. */
	const gateReadout = (container: HTMLElement) =>
		Array.from(container.querySelectorAll("span")).find((el) => el.textContent?.startsWith("Gate:"));

	it("shows the level gate on the meter's own scale, not as a raw RMS", () => {
		// meter = sqrt(rms * 20): 0.001 lands at 14%, a number the user can
		// compare against the bar. "0.001" on a 0–100% meter cannot be.
		const { container } = render(() => <DictationSettings />);
		expect(gateReadout(container)?.textContent).toBe("Gate: 14%");
	});

	it("converts a slider move back to a raw RMS before saving", () => {
		const { container } = render(() => <DictationSettings />);
		const slider = Array.from(container.querySelectorAll<HTMLInputElement>('input[type="range"]')).find(
			(el) => el.max === "50",
		);
		expect(slider).toBeDefined();

		fireEvent.input(slider as HTMLInputElement, { target: { value: "30" } });

		// 0.30^2 / 20 = 0.0045
		expect(mockStore.setRmsThreshold).toHaveBeenCalledWith(0.0045);
	});

	it("reports why the last recording was rejected", () => {
		mockStore.state.lastSkipReason = "no speech detected (no_speech 0.91 > 0.60)";
		const { container } = render(() => <DictationSettings />);
		expect(container.textContent).toContain("no speech detected (no_speech 0.91 > 0.60)");
	});

	it("starts a test recording from the panel", async () => {
		mockStore.startRecording.mockResolvedValue(undefined);

		const { getByText } = render(() => <DictationSettings />);
		fireEvent.click(getByText("Start test recording"));
		await Promise.resolve();

		expect(mockStore.startRecording).toHaveBeenCalledOnce();
	});

	it("keeps the test transcript in the panel instead of sending it anywhere", async () => {
		// The point of the harness: tuning must not fire text at whatever
		// terminal happens to be focused behind the settings panel.
		mockStore.state.recording = true;
		mockStore.stopRecording.mockResolvedValue({
			text: "run the tests",
			skip_reason: null,
			duration_s: 1.2,
			truncated_s: 0,
		});

		const { container, getByText } = render(() => <DictationSettings />);
		fireEvent.click(getByText("Stop test"));
		await new Promise((r) => setTimeout(r, 0));

		expect(mockStore.stopRecording).toHaveBeenCalledOnce();
		expect(mockStore.injectText).not.toHaveBeenCalled();
		expect(container.textContent).toContain("run the tests");
	});
});

describe("DictationSettings – Spoken replies", () => {
	const asset = (id: string, language: string | null, state: string) => ({
		id,
		display_name: id,
		kind: language ? "language" : "runtime",
		language,
		voices: [],
		download_bytes: 1_000_000,
		state,
	});

	/** The speech rows, which follow the Whisper rows in the same markup. */
	const speechRows = (container: HTMLElement) =>
		Array.from(container.querySelectorAll(".modelRow")).slice(mockStore.state.models.length);

	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockResolvedValue("not_determined");
		mockStore.state.language = "it";
		mockStore.state.speechDownloads = {};
		mockStore.state.speechAssets = [
			asset("runtime", null, "ready"),
			asset("italian", "it", "ready"),
			asset("english", "en", "ready"),
		];
	});

	it("marks only the language replies are spoken in as active, like the selected Whisper model", () => {
		const { container } = render(() => <DictationSettings />);
		const rows = speechRows(container);
		const active = rows.filter((row) => row.classList.contains("active")).map((row) => row.textContent);
		expect(active).toHaveLength(1);
		expect(active[0]).toContain("italian");
		expect(active[0]).toContain("Active");
		expect(rows.every((row) => row.textContent?.includes("Downloaded"))).toBe(true);
	});

	it("keeps catalogue voices out of the download list, and never lets one stand in for its language", () => {
		// `get_speech_assets` also returns every catalogue voice (`kind: "voice"`,
		// with its language's code). Voices belong in the voice list, not among
		// the runtime and language downloads.
		mockStore.state.speechAssets = [
			{ ...asset("voice-italian-jean", "it", "absent"), kind: "voice", voice: "jean" },
			...mockStore.state.speechAssets,
		];
		const { container } = render(() => <DictationSettings />);
		const rows = Array.from(container.querySelectorAll("[data-speech-downloads] .modelRow"));
		expect(rows).toHaveLength(3);
		expect(rows.some((row) => row.textContent?.includes("voice-italian-jean"))).toBe(false);
		const active = rows.filter((row) => row.classList.contains("active"));
		expect(active.map((row) => row.textContent)).toEqual([expect.stringContaining("italian")]);
	});

	it("cancels a running download from a × control, not a text button", () => {
		mockStore.state.speechDownloads = { english: 40 };
		const { container } = render(() => <DictationSettings />);
		const english = speechRows(container).find((row) => row.textContent?.includes("english"))!;
		const cancel = english.querySelector('button[title="Cancel"]') as HTMLButtonElement;
		expect(cancel.textContent).toBe("×");
		fireEvent.click(cancel);
		expect(mockStore.cancelSpeechDownload).toHaveBeenCalledWith("english");
	});
});

describe("DictationSettings – Voice library", () => {
	const voiceAsset = (voice: string, language: string, state: string) => ({
		id: `voice-${language}-${voice}`,
		display_name: voice,
		kind: "voice",
		language,
		voice,
		voices: [],
		download_bytes: 6_000_000,
		state,
	});

	const group = (container: HTMLElement, name: string) =>
		container.querySelector(`[data-voice-group="${name}"]`) as HTMLElement | null;
	const button = (root: ParentNode, text: string) =>
		Array.from(root.querySelectorAll("button")).find((b) => b.textContent?.trim() === text);
	const slider = (container: HTMLElement, label: string) =>
		Array.from(container.querySelectorAll("label"))
			.find((el) => el.textContent === label)
			?.parentElement?.querySelector('input[type="range"]') as HTMLInputElement;
	const voiceSelect = (container: HTMLElement) =>
		Array.from(container.querySelectorAll("select")).find((el) =>
			Array.from(el.options).some((o) => o.textContent === "Default for this language"),
		) as HTMLSelectElement;

	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockResolvedValue("not_determined");
		mockStore.state.language = "it";
		mockStore.state.speechVoice = "";
		mockStore.state.speechVolumeDb = -18;
		mockStore.state.speechLevelling = 0.67;
		mockStore.state.speechDownloads = { "voice-it-marco": 30 };
		mockStore.state.speechAssets = [
			{
				id: "italian",
				display_name: "Italian",
				kind: "language",
				language: "it",
				voices: ["giovanni"],
				download_bytes: 1,
				state: "ready",
			},
			voiceAsset("jean", "it", "ready"),
			voiceAsset("alba", "it", "absent"),
			voiceAsset("marco", "it", "downloading"),
			voiceAsset("lola", "es", "absent"),
		];
		mockStore.state.speechVoices = [
			{ id: "giovanni", source: "default" },
			{ id: "jean", source: "downloaded" },
			{ id: "my_voice", source: "user" },
		];
	});

	it("reads the voices of the language replies are spoken in", () => {
		render(() => <DictationSettings />);
		expect(mockStore.refreshSpeechVoices).toHaveBeenCalledWith("it");
	});

	it("groups the language's voices into Installed, Downloadable and Yours", () => {
		const { container } = render(() => <DictationSettings />);
		expect(group(container, "installed")?.textContent).toContain("jean");
		const downloadable = group(container, "downloadable") as HTMLElement;
		expect(downloadable.textContent).toContain("alba");
		expect(downloadable.textContent).toContain("marco");
		expect(downloadable.textContent).toContain("30%");
		expect(group(container, "yours")?.textContent).toContain("my_voice");
		// Another language's voice is in none of them.
		expect(container.textContent).not.toContain("lola");
	});

	it("downloads a voice from its own row", () => {
		const { container } = render(() => <DictationSettings />);
		const row = Array.from(group(container, "downloadable")!.querySelectorAll(".modelRow")).find((r) =>
			r.textContent?.includes("alba"),
		)!;
		fireEvent.click(button(row, "Download")!);
		expect(mockStore.downloadSpeechAsset).toHaveBeenCalledWith("voice-it-alba");
	});

	it("imports a voice file into the language and deletes a user voice", async () => {
		const { container } = render(() => <DictationSettings />);
		const yours = group(container, "yours")!;
		expect(button(yours, "Add voice file…")).toBeDefined();
		const input = yours.querySelector('input[type="file"]') as HTMLInputElement;
		const file = new File([new Uint8Array([1])], "mine.safetensors");
		fireEvent.change(input, { target: { files: [file] } });
		await Promise.resolve();
		expect(mockStore.importSpeechVoice).toHaveBeenCalledWith("it", file);

		fireEvent.click(yours.querySelector('button[title="Delete this voice file"]') as HTMLButtonElement);
		expect(mockStore.deleteSpeechVoice).toHaveBeenCalledWith("it", "my_voice");
	});

	it("shows why an imported file was refused", async () => {
		mockStore.importSpeechVoice.mockResolvedValueOnce("the voice file is over the 64 MB limit");
		const { container, findByText } = render(() => <DictationSettings />);
		const input = group(container, "yours")!.querySelector('input[type="file"]') as HTMLInputElement;
		fireEvent.change(input, { target: { files: [new File([new Uint8Array([1])], "big.safetensors")] } });
		expect(await findByText("the voice file is over the 64 MB limit")).toBeDefined();
	});

	it("offers only voices that can speak now in the voice picker", () => {
		const { container } = render(() => <DictationSettings />);
		const values = Array.from(voiceSelect(container).options).map((o) => o.value);
		expect(values).toEqual(["", "giovanni", "jean", "my_voice"]);
	});

	it("offers no voice in the picker while the language itself is not downloaded", () => {
		// No voice speaks without the language's model, so the shipped voice
		// the catalogue names must not stand in for an empty answer.
		mockStore.state.speechAssets[0] = {
			id: "italian",
			display_name: "Italian",
			kind: "language",
			language: "it",
			voices: ["giovanni"],
			download_bytes: 1,
			state: "absent",
		};
		mockStore.state.speechVoices = [];
		const { container } = render(() => <DictationSettings />);
		const values = Array.from(voiceSelect(container).options).map((o) => o.value);
		expect(values).toEqual([""]);
	});

	it("keeps the Downloadable voices collapsed behind a disclosure that shows how many there are", () => {
		// A language offers two dozen voices; listed open they push the volume
		// sliders out of sight.
		const { container } = render(() => <DictationSettings />);
		const disclosure = group(container, "downloadable") as HTMLDetailsElement;
		expect(disclosure.tagName).toBe("DETAILS");
		expect(disclosure.open).toBe(false);
		// A native <summary> is focusable and toggles on Enter and Space.
		const summary = disclosure.querySelector(":scope > summary") as HTMLElement;
		expect(summary.textContent).toContain("Downloadable");
		expect(summary.textContent).toContain("2");
		fireEvent.click(summary);
		expect(disclosure.open).toBe(true);
	});

	it("Listen previews the selected voice and shows a refusal inline", async () => {
		mockStore.state.speechVoice = "jean";
		mockStore.previewSpeechVoice.mockResolvedValueOnce("a reply is being spoken; try again when it ends");
		const { container, findByText } = render(() => <DictationSettings />);
		fireEvent.click(button(container, "Listen")!);
		expect(mockStore.previewSpeechVoice).toHaveBeenCalledWith("it", "jean");
		expect(await findByText("a reply is being spoken; try again when it ends")).toBeDefined();
	});

	it("saves Voice volume and Levelling when the drag is released, not while it moves", () => {
		const { container } = render(() => <DictationSettings />);
		const volume = slider(container, "Voice volume");
		expect(volume.value).toBe("-18");
		fireEvent.input(volume, { target: { value: "-24" } });
		expect(mockStore.setSpeechVolumeDb).not.toHaveBeenCalled();
		expect(volume.parentElement?.textContent).toContain("-24 dB");
		fireEvent.change(volume, { target: { value: "-24" } });
		expect(mockStore.setSpeechVolumeDb).toHaveBeenCalledWith(-24);

		const levelling = slider(container, "Levelling");
		expect(levelling.value).toBe("67");
		fireEvent.input(levelling, { target: { value: "0" } });
		expect(levelling.parentElement?.textContent).toContain("Off");
		fireEvent.change(levelling, { target: { value: "0" } });
		expect(mockStore.setSpeechLevelling).toHaveBeenCalledWith(0);
	});
});

describe("DictationSettings – Language without spoken replies", () => {
	const languageAsset = (language: string) => ({
		id: language,
		display_name: language,
		kind: "language",
		language,
		voices: [],
		download_bytes: 1_000_000,
		state: "ready",
	});

	const languageSelect = (container: HTMLElement) =>
		Array.from(container.querySelectorAll("select")).find((select) =>
			Array.from(select.options).some((option) => option.value === "ja"),
		)!;
	const optionText = (container: HTMLElement, code: string) =>
		Array.from(languageSelect(container).options).find((option) => option.value === code)?.textContent;
	const noVoiceHint = (container: HTMLElement) =>
		Array.from(container.querySelectorAll("p")).find((p) => p.textContent?.includes("will not be spoken"));

	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockResolvedValue("not_determined");
		mockStore.state.speechDownloads = {};
		mockStore.state.speechAssets = [languageAsset("it"), languageAsset("en")];
	});

	it("marks only the languages no speech bundle ships for, never auto", () => {
		mockStore.state.language = "auto";
		const { container } = render(() => <DictationSettings />);
		expect(optionText(container, "ja")).toBe("Japanese — no spoken replies");
		expect(optionText(container, "it")).toBe("Italian");
		expect(optionText(container, "en")).toBe("English");
		expect(optionText(container, "auto")).toBe("Auto-detect");
	});

	it("warns under the select when the chosen language cannot be spoken back", () => {
		mockStore.state.language = "ja";
		const { container } = render(() => <DictationSettings />);
		const hint = noVoiceHint(container);
		expect(hint).toBeDefined();
		expect(hint!.textContent).toContain("Auto-detect");
		expect(hint!.classList.contains("hint")).toBe(true);
	});

	it("does not warn for a language with a speech bundle, or for auto", () => {
		mockStore.state.language = "it";
		const first = render(() => <DictationSettings />);
		expect(noVoiceHint(first.container)).toBeUndefined();
		first.unmount();
		mockStore.state.language = "auto";
		const { container } = render(() => <DictationSettings />);
		expect(noVoiceHint(container)).toBeUndefined();
	});

	it("claims nothing before the speech catalogue has loaded", () => {
		mockStore.state.language = "ja";
		mockStore.state.speechAssets = [];
		const { container } = render(() => <DictationSettings />);
		expect(optionText(container, "ja")).toBe("Japanese");
		expect(noVoiceHint(container)).toBeUndefined();
	});
});

describe("DictationSettings – layout", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockResolvedValue("not_determined");
		mockStore.getDefaultHandsFreeStartNotice.mockResolvedValue("Built-in notice from Rust");
		mockStore.state.notifyModelOnHandsFree = true;
		mockStore.state.handsFreeStartNotice = "";
	});

	/** Which section heading each label renders under, in DOM order. */
	const sectionOf = (container: HTMLElement): Record<string, string> => {
		const owner: Record<string, string> = {};
		let section = "";
		for (const node of Array.from(container.querySelectorAll("h3, label"))) {
			if (node.tagName === "H3") section = node.textContent ?? "";
			else owner[node.textContent ?? ""] = section;
		}
		return owner;
	};

	it("keeps speech-to-text and text-to-speech in separate titled sections", () => {
		const { container } = render(() => <DictationSettings />);
		const headings = Array.from(container.querySelectorAll("h3")).map((h) => h.textContent);
		expect(headings).toEqual([
			"Dictation",
			"Speech recognition",
			"Auto-Corrections",
			"Hands-free conversation",
			"Spoken replies",
		]);
	});

	it("puts each side's advanced settings inside its own section, never in a shared one", () => {
		// The speech gates tune recognition, so they belong to it; nothing that
		// tunes recognition may land under the text-to-speech heading.
		mockStore.state.language = "it";
		mockStore.state.speechAssets = [
			{
				id: "italian",
				display_name: "Italian",
				kind: "language",
				language: "it",
				voices: ["a", "b"],
				download_bytes: 1,
				state: "ready",
			},
		];
		const owner = sectionOf(render(() => <DictationSettings />).container);
		for (const label of [
			"Input device",
			"Whisper Model",
			"Language",
			"Voice tuning",
			"Level gate",
			"Speech confidence gate",
		]) {
			expect(owner[label]).toBe("Speech recognition");
		}
		expect(owner.Voice).toBe("Spoken replies");
	});

	it("keeps the hands-free switches inside the hands-free section", () => {
		// The "notify model" toggle only matters while a conversation runs, so it
		// must not sit among the push-to-talk settings at the top.
		const owner = sectionOf(render(() => <DictationSettings />).container);
		for (const label of [
			"Activation phrase",
			"Hold-back before sending",
			"Earcons",
			"Notify model when hands-free changes",
			"Start notice",
		]) {
			expect(owner[label]).toBe("Hands-free conversation");
		}
	});

	it("turns the hands-free earcons off from their own toggle", () => {
		const { container } = render(() => <DictationSettings />);
		const group = Array.from(container.querySelectorAll("label")).find((l) => l.textContent === "Earcons")
			?.parentElement as HTMLElement;
		const toggle = group.querySelector('input[type="checkbox"]') as HTMLInputElement;
		expect(toggle.checked).toBe(true);
		fireEvent.change(toggle, { target: { checked: false } });
		expect(mockStore.setHandsFreeEarcons).toHaveBeenCalledWith(false);
	});

	it("suggests “computer” as the activation phrase", () => {
		const { container } = render(() => <DictationSettings />);
		const group = Array.from(container.querySelectorAll("label")).find((l) => l.textContent === "Activation phrase")
			?.parentElement as HTMLElement;
		expect((group.querySelector('input[type="text"]') as HTMLInputElement).placeholder).toBe("computer");
		expect(group.textContent).toContain("Try “computer”");
	});

	describe("start notice", () => {
		const noticeField = (container: HTMLElement) => container.querySelector("textarea");

		it("shows the built-in text from Rust as the placeholder, never a frontend copy", async () => {
			const { container } = render(() => <DictationSettings />);
			await Promise.resolve();
			await Promise.resolve();
			expect(mockStore.getDefaultHandsFreeStartNotice).toHaveBeenCalledOnce();
			expect(noticeField(container)?.placeholder).toBe("Built-in notice from Rust");
		});

		it("saves an edited notice", () => {
			const { container } = render(() => <DictationSettings />);
			fireEvent.change(noticeField(container) as HTMLTextAreaElement, { target: { value: "Speak Italian." } });
			expect(mockStore.setHandsFreeStartNotice).toHaveBeenCalledWith("Speak Italian.");
		});

		it("resets a custom notice, and offers no reset when the default is already in use", () => {
			mockStore.state.handsFreeStartNotice = "Custom";
			const custom = render(() => <DictationSettings />);
			fireEvent.click(custom.getByText("Reset to default"));
			expect(mockStore.resetHandsFreeStartNotice).toHaveBeenCalledOnce();
			custom.unmount();

			mockStore.state.handsFreeStartNotice = "";
			const builtIn = render(() => <DictationSettings />);
			expect((builtIn.getByText("Reset to default") as HTMLButtonElement).disabled).toBe(true);
		});

		it("is hidden while the model is not notified, because nothing would send it", () => {
			mockStore.state.notifyModelOnHandsFree = false;
			const { container } = render(() => <DictationSettings />);
			expect(noticeField(container)).toBeNull();
		});

		it("still renders when Rust cannot supply the default text", async () => {
			mockStore.getDefaultHandsFreeStartNotice.mockRejectedValue(new Error("unknown command"));
			const { container } = render(() => <DictationSettings />);
			await Promise.resolve();
			await Promise.resolve();
			expect(noticeField(container)?.placeholder).toBe("");
		});
	});
});

describe("DictationSettings – expert controls", () => {
	// The serialized `DictationConfig::default()` values these controls compare against.
	const DEFAULTS = {
		app: {},
		notifications: {},
		agent_settings: {},
		dictation: {
			long_press_ms: 400,
			auto_send: true,
			device: null,
			rms_threshold: 0.001,
			no_speech_threshold: 0.6,
			hands_free_hold_back_ms: 1500,
			hands_free_notify_model: true,
			hands_free_start_notice: "",
		},
	};

	type State = typeof mockStore.state;
	/** label → the store field that holds its value and a non-default value for it */
	const EXPERT: Array<[string, keyof State, unknown]> = [
		["Long-press threshold", "longPressMs", 600],
		["Auto-send", "autoSend", false],
		["Input device", "selectedDevice", "USB Mic"],
		["Level gate", "rmsThreshold", 0.01],
		["Speech confidence gate", "noSpeechThreshold", 0.8],
		["Hold-back before sending", "handsFreeHoldBackMs", 0],
		["Notify model when hands-free changes", "notifyModelOnHandsFree", false],
		["Start notice", "handsFreeStartNotice", "Speak Italian."],
	];
	const BASIC = ["Enable Dictation", "Hotkey", "Whisper Model", "Language", "Activation phrase", "Earcons"];

	const hasLabel = (container: HTMLElement, text: string) =>
		[...container.querySelectorAll("label")].some((el) => el.textContent === text);
	let saved: State;

	beforeEach(async () => {
		vi.clearAllMocks();
		saved = { ...mockStore.state };
		uiStore.setSettingsExpertMode(false);
		mockInvoke.mockImplementation((cmd: string) =>
			Promise.resolve(cmd === "get_config_defaults" ? DEFAULTS : "not_determined"),
		);
		await settingsExpertStore.open();
	});

	afterEach(() => {
		Object.assign(mockStore.state, saved);
		settingsExpertStore._resetForTests();
		uiStore.setSettingsExpertMode(false);
	});

	it("hides every expert control at its default in basic mode and keeps the basic ones", () => {
		const { container } = render(() => <DictationSettings />);
		for (const [label] of EXPERT) expect(hasLabel(container, label), label).toBe(false);
		for (const label of BASIC) expect(hasLabel(container, label), label).toBe(true);
	});

	it.each(EXPERT)("shows %s in basic mode once it differs from the default", (label, field, modified) => {
		(mockStore.state as Record<string, unknown>)[field] = modified;
		const { container } = render(() => <DictationSettings />);
		expect(hasLabel(container, label)).toBe(true);
	});

	it("keeps Start notice visible after Reset to default puts it back at the default", () => {
		mockStore.state.handsFreeStartNotice = "Speak Italian.";
		const { getByText } = render(() => <DictationSettings />);
		fireEvent.click(getByText("Reset to default"));
		// A click is not an input/change event, so the button must pin the control itself.
		expect(settingsExpertStore.isVisible("dictation.hands_free_start_notice", "")).toBe(true);
	});

	it("shows every expert control at its default in expert mode", async () => {
		uiStore.setSettingsExpertMode(true);
		const { container } = render(() => <DictationSettings />);
		await waitFor(() => {
			for (const [label] of EXPERT) expect(hasLabel(container, label), label).toBe(true);
		});
	});
});
