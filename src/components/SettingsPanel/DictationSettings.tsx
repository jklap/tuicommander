import { type Component, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import type { ModelInfo, SpeechAsset } from "../../stores/dictation";
import { dictationStore, WHISPER_LANGUAGES } from "../../stores/dictation";
import { terminalsStore } from "../../stores/terminals";
import { isTauri } from "../../transport";
import { cx } from "../../utils";
import { KeyComboCapture } from "../shared/KeyComboCapture";
import d from "./DictationSettings.module.css";
import { SettingSlider } from "./SettingFields";
import s from "./Settings.module.css";

/**
 * Mirrors `audio.rs`: `meter_level = sqrt(rms * 20)`.
 *
 * The gate is a raw RMS and the meter is that curve, so a threshold shown in raw
 * units cannot be compared against the bar the user is watching. Both are drawn
 * on the meter's scale instead, and only converted back on save.
 */
const RMS_METER_SCALE = 20;

function rmsToMeter(rms: number): number {
	return Math.min(1, Math.sqrt(Math.max(0, rms) * RMS_METER_SCALE));
}

function meterToRms(meter: number): number {
	return (meter * meter) / RMS_METER_SCALE;
}

/** Single model row in the model selector list */
const ModelRow: Component<{ model: ModelInfo }> = (props) => {
	const isSelected = () => dictationStore.state.selectedModel === props.model.name;
	const isDownloading = () =>
		dictationStore.state.downloading && dictationStore.state.selectedModel === props.model.name;

	const sizeLabel = () =>
		props.model.downloaded && props.model.actual_size_mb > 0
			? `${props.model.actual_size_mb} MB`
			: `~${props.model.size_hint_mb} MB`;

	return (
		<div class={cx(d.modelRow, isSelected() && d.active)}>
			<div class={d.modelInfo}>
				<span class={d.modelName}>{props.model.display_name}</span>
				<span class={d.modelSize}>{sizeLabel()}</span>
			</div>
			<Show when={!isDownloading()}>
				<span class={cx(d.modelBadge, props.model.downloaded && d.downloaded)}>
					{props.model.downloaded
						? t("dictation.downloaded", "Downloaded")
						: t("dictation.notDownloaded", "Not Downloaded")}
				</span>
			</Show>
			<div class={d.modelActions}>
				<Show when={props.model.downloaded && !isSelected()}>
					<button class={d.modelSelect} onClick={() => dictationStore.setModel(props.model.name)}>
						{t("dictation.use", "Use")}
					</button>
				</Show>
				<Show when={props.model.downloaded && isSelected()}>
					<span class={d.modelActiveLabel}>{t("dictation.active", "Active")}</span>
				</Show>
				<Show when={!props.model.downloaded && !isDownloading()}>
					<button class={d.modelDownload} onClick={() => dictationStore.downloadModel(props.model.name)}>
						{t("dictation.download", "Download")}
					</button>
				</Show>
				<Show when={isDownloading()}>
					<div class={d.downloadProgress}>
						<div class={d.progressBar}>
							<div
								class={d.progressFill}
								style={{ transform: `scaleX(${dictationStore.state.downloadPercent / 100})` }}
							/>
						</div>
						<span class={d.progressText}>{dictationStore.state.downloadPercent}%</span>
					</div>
				</Show>
				<Show when={props.model.downloaded}>
					<button
						class={d.modelDelete}
						onClick={() => dictationStore.deleteModel(props.model.name)}
						title={t("dictation.deleteModel", "Delete model")}
					>
						&times;
					</button>
				</Show>
			</div>
		</div>
	);
};

/** Dictation settings tab for the Settings panel */
export const DictationSettings: Component = () => {
	const [newFrom, setNewFrom] = createSignal("");
	const [newTo, setNewTo] = createSignal("");

	// Load data on mount. Auto-detect devices only if mic is already authorized
	// (avoids triggering the macOS TCC permission dialog unexpectedly).
	onMount(async () => {
		dictationStore.refreshConfig();
		dictationStore.refreshStatus();
		dictationStore.refreshCorrections();
		dictationStore.refreshModels();
		try {
			const perm = await invoke<string>("check_microphone_permission");
			if (perm === "authorized") {
				dictationStore.refreshDevices();
			} else if (perm === "denied" || perm === "restricted") {
				appLogger.warn(
					"dictation",
					`Microphone access ${perm} — grant permission in System Settings > Privacy > Microphone`,
				);
			}
		} catch {
			appLogger.warn("dictation", "Failed to check microphone permission");
		}
	});

	const handleAddCorrection = () => {
		const from = newFrom().trim();
		const to = newTo().trim();
		if (!from || !to) return;

		const updated = { ...dictationStore.state.corrections, [from]: to };
		dictationStore.saveCorrections(updated);
		setNewFrom("");
		setNewTo("");
	};

	const handleRemoveCorrection = (key: string) => {
		const updated = { ...dictationStore.state.corrections };
		delete updated[key];
		dictationStore.saveCorrections(updated);
	};

	const handleExportCorrections = () => {
		const json = JSON.stringify(dictationStore.state.corrections, null, 2);
		const blob = new Blob([json], { type: "application/json" });
		const url = URL.createObjectURL(blob);
		const a = document.createElement("a");
		a.href = url;
		a.download = "dictation-corrections.json";
		a.click();
		URL.revokeObjectURL(url);
	};

	const handleImportCorrections = () => {
		const input = document.createElement("input");
		input.type = "file";
		input.accept = ".json";
		input.onchange = async () => {
			const file = input.files?.[0];
			if (!file) return;
			try {
				const text = await file.text();
				const map = JSON.parse(text);
				if (typeof map === "object" && map !== null) {
					dictationStore.saveCorrections(map as Record<string, string>);
				}
			} catch {
				appLogger.error("dictation", "Failed to import corrections file");
			}
		};
		input.click();
	};

	return (
		<div class={s.section}>
			<h3>{t("dictation.title", "Dictation Settings")}</h3>

			{/* Enable toggle */}
			<div class={s.group}>
				<label>{t("dictation.enableLabel", "Enable Dictation")}</label>
				<div class={s.toggle}>
					<input
						type="checkbox"
						checked={dictationStore.state.enabled}
						onChange={(e) => dictationStore.setEnabled(e.currentTarget.checked)}
					/>
					<span>{t("dictation.enableHint", "Enable voice-to-text dictation")}</span>
				</div>
			</div>

			{/* Model selector */}
			<div class={s.group}>
				<label>{t("dictation.modelLabel", "Whisper Model")}</label>
				<p class={s.hint} style={{ "margin-bottom": "8px" }}>
					{t("dictation.modelHint", "Choose a model. Larger models are more accurate but slower.")}
				</p>
				<div class={d.modelList}>
					<For each={dictationStore.state.models}>{(model: ModelInfo) => <ModelRow model={model} />}</For>
				</div>
			</div>

			{/* Hotkey — a global hotkey belongs to the machine running
			    TUICommander, so a browser tab has none to configure. */}
			<Show when={isTauri()}>
				<div class={s.group}>
					<label>{t("dictation.hotkeyLabel", "Hotkey")}</label>
					<div class={d.hotkeyRow}>
						<KeyComboCapture
							value={dictationStore.state.hotkey}
							onChange={(combo) => dictationStore.setHotkey(combo)}
							placeholder={t("dictation.hotkeyPlaceholder", "Press a key combination...")}
							onCapturingChange={(capturing) => dictationStore.setCapturingHotkey(capturing)}
						/>
					</div>
					<p class={s.hint}>
						{t(
							"dictation.hotkeyHint",
							"Hold the hotkey to start recording, release to stop. Short presses pass through as normal input.",
						)}
					</p>
				</div>

				{/* Long-press threshold */}
				<SettingSlider
					label={t("dictation.longPressLabel", "Long-press threshold")}
					value={dictationStore.state.longPressMs}
					onChange={(v) => dictationStore.setLongPressMs(v)}
					min={0}
					max={1000}
					step={50}
					formatValue={(v) => (v === 0 ? t("dictation.instant", "Instant") : `${v}ms`)}
					hint={t(
						"dictation.longPressHint",
						"How long to hold the key before dictation starts. 0 = instant (no short-press pass-through), higher = fewer accidental triggers.",
					)}
				/>
			</Show>

			{/* Auto-send */}
			<div class={s.group}>
				<label>{t("dictation.autoSendLabel", "Auto-send")}</label>
				<div class={s.toggle}>
					<input
						type="checkbox"
						checked={dictationStore.state.autoSend}
						onChange={(e) => dictationStore.setAutoSend(e.currentTarget.checked)}
					/>
					<span>{t("dictation.autoSendHint", "Automatically press Enter after inserting transcribed text")}</span>
				</div>
			</div>

			{/* Tell the model when hands-free starts and stops */}
			<div class={s.group}>
				<label>{t("dictation.notifyModelLabel", "Notify model when hands-free changes")}</label>
				<div class={s.toggle}>
					<input
						type="checkbox"
						checked={dictationStore.state.notifyModelOnHandsFree}
						onChange={(e) => dictationStore.setNotifyModelOnHandsFree(e.currentTarget.checked)}
					/>
					<span>
						{t(
							"dictation.notifyModelHint",
							"Tell the agent when a hands-free conversation starts, so it answers out loud, and when it ends, so it goes back to text. Turning this off never leaves speech running: disarming always stops it.",
						)}
					</span>
				</div>
			</div>

			{/* Language */}
			<div class={s.group}>
				<label>{t("dictation.languageLabel", "Language")}</label>
				<select
					value={dictationStore.state.language}
					onChange={(e) => dictationStore.setLanguage(e.currentTarget.value)}
				>
					<For each={Object.entries(WHISPER_LANGUAGES)}>
						{([value, label]) => <option value={value}>{label}</option>}
					</For>
				</select>
				<p class={s.hint}>{t("dictation.languageHint", "Auto-detect works well for most languages.")}</p>
			</div>

			{/* Audio devices — this list is the *server's* hardware. A browser
			    captures from its own device, chosen by the browser's own
			    permission prompt, so offering these names there would let a
			    user pick a microphone in another building. */}
			<Show when={isTauri()}>
				<div class={s.group}>
					<label>{t("dictation.microphoneLabel", "Microphone")}</label>
					<Show
						when={dictationStore.state.devices.length > 0}
						fallback={
							<div>
								<button
									class={s.downloadBtn}
									onClick={() => dictationStore.refreshDevices()}
									style={{
										background: "var(--bg-tertiary)",
										color: "var(--fg-secondary)",
										border: "1px solid var(--border)",
									}}
								>
									{t("dictation.detectMicrophones", "Detect Microphones")}
								</button>
								<p class={s.hint}>
									{t("dictation.detectMicrophonesHint", "Triggers macOS microphone permission dialog.")}
								</p>
							</div>
						}
					>
						<select
							value={dictationStore.state.selectedDevice ?? ""}
							onChange={(e) => {
								const val = e.currentTarget.value;
								dictationStore.setDevice(val === "" ? null : val);
							}}
						>
							<option value="">{t("dictation.systemDefault", "System Default")}</option>
							<For each={dictationStore.state.devices}>
								{(device) => <option value={device.name}>{device.name}</option>}
							</For>
						</select>
						<p class={s.hint}>{t("dictation.microphoneHint", "Select the input device to use for dictation.")}</p>
					</Show>
				</div>
			</Show>

			{/* Voice tuning */}
			<VoiceTuning />

			{/* Spoken replies */}
			<SpeechSetup />

			{/* Hands-free conversation */}
			<HandsFreeControls />

			{/* Correction map */}
			<div class={s.group}>
				<label>{t("dictation.correctionsLabel", "Auto-Corrections")}</label>
				<p class={s.hint} style={{ "margin-bottom": "8px" }}>
					{t("dictation.correctionsHint", "Automatically replace dictation output. Useful for technical terms.")}
				</p>

				{/* Existing corrections */}
				<Show when={Object.keys(dictationStore.state.corrections).length > 0}>
					<div class={d.correctionsTable}>
						<div class={d.correctionsHeader}>
							<span>{t("dictation.correctionsFrom", "From")}</span>
							<span>{t("dictation.correctionsTo", "To")}</span>
							<span />
						</div>
						<For each={Object.entries(dictationStore.state.corrections)}>
							{([from, to]) => (
								<div class={d.correctionsRow}>
									<span class={d.correctionText}>{from}</span>
									<span class={d.correctionText}>{to}</span>
									<button
										class={d.correctionDelete}
										onClick={() => handleRemoveCorrection(from)}
										title={t("dictation.removeCorrection", "Remove correction")}
									>
										&times;
									</button>
								</div>
							)}
						</For>
					</div>
				</Show>

				{/* Add new correction */}
				<div class={d.correctionAdd}>
					<input
						type="text"
						placeholder={t("dictation.correctionFromPlaceholder", "Heard text...")}
						value={newFrom()}
						onInput={(e) => setNewFrom(e.currentTarget.value)}
						onKeyDown={(e) => e.key === "Enter" && handleAddCorrection()}
					/>
					<span class={d.correctionArrow}>&rarr;</span>
					<input
						type="text"
						placeholder={t("dictation.correctionToPlaceholder", "Replace with...")}
						value={newTo()}
						onInput={(e) => setNewTo(e.currentTarget.value)}
						onKeyDown={(e) => e.key === "Enter" && handleAddCorrection()}
					/>
					<button
						class={d.correctionAddBtn}
						onClick={handleAddCorrection}
						disabled={!newFrom().trim() || !newTo().trim()}
					>
						{t("dictation.addCorrection", "Add")}
					</button>
				</div>

				{/* Import/Export */}
				<div class={s.actions} style={{ "margin-top": "8px" }}>
					<button onClick={handleImportCorrections}>{t("dictation.import", "Import")}</button>
					<button onClick={handleExportCorrections}>{t("dictation.export", "Export")}</button>
				</div>
			</div>
		</div>
	);
};

/**
 * Live harness for the two speech gates.
 *
 * Defined BELOW the panel that renders it, which is why it reads out of order.
 * `extractSettings` builds the settings search index from source order and
 * assigns each label to the nearest preceding `<h3>`; it does not follow the
 * render tree. Defined above the panel, this component's three labels sat
 * before any heading, so they counted as orphans and "Level gate" and "Speech
 * confidence gate" were unreachable from settings search. Keep it here.
 *
 * Recording here reports the transcript back into the panel instead of typing it
 * into a terminal: tuning a gate means seeing what it rejected, and a threshold
 * that swallows speech is indistinguishable from a dead microphone until the
 * skip reason is on screen.
 */
const VoiceTuning: Component = () => {
	const [testText, setTestText] = createSignal<string | null>(null);

	const recording = () => dictationStore.state.recording;
	const thresholdPercent = () => rmsToMeter(dictationStore.state.rmsThreshold) * 100;
	const levelPercent = () => dictationStore.state.audioLevel * 100;

	const toggleTest = async () => {
		if (recording()) {
			const result = await dictationStore.stopRecording();
			setTestText(result?.text.trim() || null);
			return;
		}
		setTestText(null);
		try {
			await dictationStore.startRecording();
		} catch {
			// startRecording logs the failure; lastSkipReason covers the rest.
		}
	};

	return (
		<div class={s.group}>
			<label>{t("dictation.tuningLabel", "Voice tuning")}</label>
			<p class={s.hint} style={{ "margin-bottom": "8px" }}>
				{t(
					"dictation.tuningHint",
					"Record a test phrase and watch where your voice sits against the gates. Text stays in this panel — nothing is sent to a terminal.",
				)}
			</p>

			<div class={d.tuningMeter}>
				<div class={d.tuningLevel} style={{ transform: `scaleX(${dictationStore.state.audioLevel})` }} />
				<div
					class={d.tuningThreshold}
					style={{ left: `${thresholdPercent()}%` }}
					title={t("dictation.tuningThresholdMarker", "Level gate")}
				/>
			</div>
			<div class={d.tuningReadout}>
				<span>
					{t("dictation.tuningLevelReadout", "Level")}: {Math.round(levelPercent())}%
				</span>
				<span>
					{t("dictation.tuningGateReadout", "Gate")}: {Math.round(thresholdPercent())}%
				</span>
			</div>

			<div class={s.actions} style={{ "margin-top": "8px" }}>
				<button onClick={toggleTest} disabled={dictationStore.state.processing}>
					{recording() ? t("dictation.tuningStop", "Stop test") : t("dictation.tuningStart", "Start test recording")}
				</button>
			</div>

			<Show when={dictationStore.state.partialText}>
				<p class={d.tuningPartial}>{dictationStore.state.partialText}</p>
			</Show>
			<Show when={testText()}>
				<p class={d.tuningResult}>{testText()}</p>
			</Show>
			<Show when={dictationStore.state.lastSkipReason}>
				<p class={d.tuningSkip}>
					{t("dictation.tuningSkipped", "Rejected")}: {dictationStore.state.lastSkipReason}
				</p>
			</Show>

			<SettingSlider
				label={t("dictation.rmsLabel", "Level gate")}
				value={Math.round(thresholdPercent())}
				onChange={(v) => dictationStore.setRmsThreshold(meterToRms(v / 100))}
				min={0}
				max={50}
				step={1}
				formatValue={(v) => `${v}%`}
				hint={t(
					"dictation.rmsHint",
					"Audio quieter than this never reaches Whisper. Raise it until room noise stays below the marker; lower it if quiet speech is rejected.",
				)}
			/>

			<SettingSlider
				label={t("dictation.noSpeechLabel", "Speech confidence gate")}
				value={Math.round(dictationStore.state.noSpeechThreshold * 100)}
				onChange={(v) => dictationStore.setNoSpeechThreshold(v / 100)}
				min={10}
				max={100}
				step={5}
				formatValue={(v) => (v === 100 ? t("dictation.off", "Off") : `${v}%`)}
				hint={t(
					"dictation.noSpeechHint",
					"Discards a transcript when Whisper itself reports it probably heard no speech. Lower is stricter; 100% turns the gate off.",
				)}
			/>
		</div>
	);
};

/** Bytes as the megabytes a download dialog would quote. */
function megabytes(bytes: number): string {
	return `${Math.round(bytes / 1_000_000)} MB`;
}

/**
 * Setting up the voice that speaks replies back.
 *
 * Defined below the panel for the same reason as `VoiceTuning`: the settings
 * search index is built from source order and assigns each label to the
 * nearest preceding `<h3>`.
 *
 * There is no language control here on purpose. A conversation is held in one
 * language, and that is the Whisper language above — picking a second one is
 * how you get a reply in English to a question asked in Italian. What the user
 * chooses here is which of that language's voices speaks it.
 */
const SpeechSetup: Component = () => {
	onMount(() => {
		dictationStore.refreshSpeechAssets();
	});

	const languageAsset = (): SpeechAsset | undefined =>
		dictationStore.state.speechAssets.find((asset) => asset.language === dictationStore.state.language);

	/** Which language replies are spoken in, in the user's terms. */
	const spokenLanguage = (): string => {
		const code = dictationStore.state.language;
		if (code === "auto") {
			return t("dictation.speechLanguageAuto", "Whatever Whisper hears — nothing is spoken until somebody speaks");
		}
		const asset = languageAsset();
		return asset
			? asset.display_name
			: t("dictation.speechLanguageMissing", "{lang} — no speech bundle ships for it").replace(
					"{lang}",
					WHISPER_LANGUAGES[code] ?? code,
				);
	};

	return (
		<div class={s.group}>
			<label>{t("dictation.speechLabel", "Spoken replies")}</label>
			<p class={s.hint} style={{ "margin-bottom": "8px" }}>
				{t(
					"dictation.speechHint",
					"Downloads needed to let an agent answer out loud. The runtime library is shared; each language is a separate bundle and brings its own voices.",
				)}
			</p>

			<div class={d.modelList}>
				<For each={dictationStore.state.speechAssets}>{(asset) => <SpeechAssetRow asset={asset} />}</For>
			</div>

			<div class={d.conversation}>
				<div class={d.conversationRow}>
					<span>{t("dictation.speechLanguageLabel", "Replies are spoken in")}</span>
					<span class={d.conversationValue}>{spokenLanguage()}</span>
				</div>
			</div>

			<Show when={(languageAsset()?.voices.length ?? 0) > 0}>
				<label style={{ "margin-top": "8px" }}>{t("dictation.voiceLabel", "Voice")}</label>
				<select
					value={dictationStore.state.speechVoice}
					onChange={(e) => dictationStore.setSpeechVoice(e.currentTarget.value)}
				>
					<option value="">{t("dictation.voiceDefault", "Default for this language")}</option>
					<For each={languageAsset()?.voices ?? []}>{(voice) => <option value={voice}>{voice}</option>}</For>
				</select>
				<p class={s.hint}>
					{t(
						"dictation.voiceHint",
						"Changing the voice stops any reply already being spoken — a sentence half said in one voice does not finish in another.",
					)}
				</p>
			</Show>
		</div>
	);
};

/** One catalogue entry: what it is, what state it is in, and what to do next. */
const SpeechAssetRow: Component<{ asset: SpeechAsset }> = (props) => {
	const percent = () => dictationStore.state.speechDownloads[props.asset.id];
	const downloading = () => props.asset.state === "downloading" || percent() !== undefined;
	// The bundle replies are spoken with — the counterpart of the selected
	// Whisper model, and highlighted the same way.
	const speaking = () =>
		props.asset.state === "ready" &&
		props.asset.language !== null &&
		props.asset.language === dictationStore.state.language;

	return (
		<div class={cx(d.modelRow, speaking() && d.active)}>
			<div class={d.modelInfo}>
				<span class={d.modelName}>{props.asset.display_name}</span>
				<span class={d.modelSize}>{megabytes(props.asset.download_bytes)}</span>
			</div>
			<Show when={!downloading()}>
				<span class={cx(d.modelBadge, props.asset.state === "ready" && d.downloaded)}>
					{props.asset.state === "ready"
						? t("dictation.downloaded", "Downloaded")
						: props.asset.state === "incomplete"
							? t("dictation.speechIncomplete", "Incomplete")
							: t("dictation.notDownloaded", "Not Downloaded")}
				</span>
			</Show>
			<div class={d.modelActions}>
				<Show when={downloading()}>
					<div class={d.downloadProgress}>
						<div class={d.progressBar}>
							<div class={d.progressFill} style={{ transform: `scaleX(${(percent() ?? 0) / 100})` }} />
						</div>
						<span class={d.progressText}>{percent() ?? 0}%</span>
					</div>
					<button
						class={d.modelDelete}
						onClick={() => dictationStore.cancelSpeechDownload(props.asset.id)}
						title={t("dictation.cancel", "Cancel")}
					>
						&times;
					</button>
				</Show>
				<Show when={!downloading() && speaking()}>
					<span class={d.modelActiveLabel}>{t("dictation.active", "Active")}</span>
				</Show>
				<Show when={!downloading() && props.asset.state !== "ready"}>
					<button class={d.modelDownload} onClick={() => dictationStore.downloadSpeechAsset(props.asset.id)}>
						{props.asset.state === "incomplete"
							? t("dictation.speechRepair", "Repair")
							: t("dictation.download", "Download")}
					</button>
				</Show>
				<Show when={!downloading() && props.asset.state !== "absent"}>
					<button
						class={d.modelDelete}
						onClick={() => dictationStore.deleteSpeechAsset(props.asset.id)}
						title={t("dictation.speechDelete", "Delete this download")}
					>
						&times;
					</button>
				</Show>
			</div>
		</div>
	);
};

/**
 * Starting, watching and stopping a hands-free conversation.
 *
 * Nothing here arms on mount. Opening this panel must never open the
 * microphone, and neither must starting the app: a conversation begins because
 * somebody pressed Start, and ends because somebody pressed Stop, pressed the
 * dictation hotkey, or closed the terminal it was bound to.
 *
 * Every decision below belongs to Rust — when an utterance ends, whether the
 * activation phrase opened it, when the hold-back expires, what may be spoken.
 * This renders the answers and offers the two buttons.
 *
 * There is deliberately **no** browser branch here, and since 832-e730 that is
 * because none is needed: a browser tab arms under its own owner name and holds
 * the conversation through its own microphone and speaker, so the same two
 * buttons do the same thing on both transports. What differs is which hardware
 * the store opens before arming, and that decision lives in
 * `dictation.ts armHandsFree` rather than in a control.
 */
const HandsFreeControls: Component = () => {
	const [target, setTarget] = createSignal(terminalsStore.getActive()?.sessionId ?? "");

	// Polled rather than pushed: hands-free state has no SSE arm yet, and a
	// panel that shows a stale phase is worse than one that lags a beat. Only
	// while this panel is open — see `onCleanup`.
	let timer: ReturnType<typeof setInterval> | null = null;
	onMount(() => {
		dictationStore.refreshHandsFree();
		dictationStore.refreshSpeechStatus();
		timer = setInterval(() => {
			dictationStore.refreshHandsFree();
			dictationStore.refreshSpeechStatus();
		}, 500);
	});
	onCleanup(() => {
		if (timer) clearInterval(timer);
	});

	const status = () => dictationStore.state.handsFree;
	const speech = () => dictationStore.state.speech;
	const armed = () => status()?.armed === true;

	/** Terminals that have a live PTY session, which is what can be bound. */
	const targets = () =>
		terminalsStore
			.getIds()
			.map((id) => terminalsStore.get(id))
			.filter((term): term is NonNullable<typeof term> => !!term?.sessionId);

	/** The phase in the user's words, and whether it needs attention. */
	const phaseLabel = (): string => {
		switch (status()?.phase) {
			case "waiting":
				return t("dictation.phaseWaiting", "Listening");
			case "capturing":
				return t("dictation.phaseCapturing", "Hearing you");
			case "transcribing":
				return t("dictation.phaseTranscribing", "Transcribing");
			case "holding_back":
				return t("dictation.phaseHoldingBack", "About to send");
			case "delivered":
				return t("dictation.phaseDelivered", "Sent");
			case "error":
				return t("dictation.phaseError", "Error");
			default:
				return t("dictation.phaseDisarmed", "Stopped");
		}
	};

	/** What the speaker is doing, or empty when it is doing nothing. */
	const speakingLabel = (): string => {
		const current = speech();
		if (!current) return "";
		if (current.speaking) return t("dictation.speechPlaying", "Playing a reply");
		if (current.rendering) return t("dictation.speechRendering", "Synthesising a reply");
		if (current.queued > 0)
			return t("dictation.speechQueued", "{n} replies waiting").replace("{n}", String(current.queued));
		return "";
	};

	const start = async () => {
		const sessionId = target();
		if (!sessionId) return;
		await dictationStore.armHandsFree(sessionId);
	};

	return (
		<div class={s.group}>
			<label>{t("dictation.handsFreeLabel", "Hands-free conversation")}</label>
			<p class={s.hint} style={{ "margin-bottom": "8px" }}>
				{t(
					"dictation.handsFreeHint",
					"Push-to-talk is the hotkey above: hold it, speak, release. Hands-free is the other mode — it binds one terminal, keeps the microphone open and sends each utterance by itself. The hotkey stops it.",
				)}
			</p>

			<div class={s.actions}>
				<Show
					when={armed()}
					fallback={
						<>
							<select value={target()} onChange={(e) => setTarget(e.currentTarget.value)}>
								<option value="">{t("dictation.handsFreeNoTarget", "Choose a terminal…")}</option>
								<For each={targets()}>{(term) => <option value={term.sessionId ?? ""}>{term.name}</option>}</For>
							</select>
							<button onClick={start} disabled={!target()}>
								{t("dictation.handsFreeStart", "Start conversation")}
							</button>
						</>
					}
				>
					<button onClick={() => dictationStore.disarmHandsFree()}>
						{t("dictation.handsFreeStop", "Stop conversation")}
					</button>
				</Show>
			</div>

			<Show when={status()}>
				{(current) => (
					<div class={d.conversation}>
						<div class={d.conversationRow}>
							<span>{t("dictation.handsFreeState", "State")}</span>
							<span
								class={cx(
									d.phase,
									current().phase === "error" && d.failed,
									current().armed && current().phase !== "error" && d.live,
								)}
							>
								{phaseLabel()}
							</span>
						</div>
						<Show when={current().sessionId}>
							<div class={d.conversationRow}>
								<span>{t("dictation.handsFreeTarget", "Bound terminal")}</span>
								<span class={d.conversationValue}>
									{terminalsStore.get(terminalsStore.getTerminalForSession(current().sessionId ?? "") ?? "")?.name ??
										current().sessionId}
								</span>
							</div>
						</Show>
						<Show when={current().owner}>
							<div class={d.conversationRow}>
								<span>{t("dictation.handsFreeOwner", "Audio from")}</span>
								<span class={d.conversationValue}>{current().owner}</span>
							</div>
						</Show>
						<Show when={speakingLabel()}>
							<div class={d.conversationRow}>
								<span>{t("dictation.handsFreeSpeaker", "Speaker")}</span>
								<span class={d.conversationValue}>{speakingLabel()}</span>
							</div>
						</Show>
						<Show when={current().armed && speech() && !speech()?.available}>
							<div class={d.conversationRow}>
								<span>{t("dictation.handsFreeNoVoice", "Cannot speak")}</span>
								<span class={d.conversationValue}>{speech()?.unavailableReason}</span>
							</div>
						</Show>
						<Show when={current().pendingText}>
							<p class={d.conversationPending}>
								{t("dictation.handsFreePending", "About to send")}: {current().pendingText}
							</p>
						</Show>
						<Show when={current().error ?? dictationStore.state.handsFreeError}>
							<p class={d.conversationError}>{current().error ?? dictationStore.state.handsFreeError}</p>
						</Show>
					</div>
				)}
			</Show>

			<label style={{ "margin-top": "8px" }}>{t("dictation.activationPhraseLabel", "Activation phrase")}</label>
			<input
				type="text"
				value={dictationStore.state.handsFreeActivationPhrase}
				placeholder={t("dictation.activationPhrasePlaceholder", "Leave empty to send every utterance")}
				onChange={(e) => dictationStore.setHandsFreeActivationPhrase(e.currentTarget.value)}
			/>
			<p class={s.hint}>
				{t(
					"dictation.activationPhraseHint",
					"When set, only speech that opens with this phrase is sent, and the phrase itself is removed first. The match runs on this machine, so unrelated speech never leaves it.",
				)}
			</p>

			<SettingSlider
				label={t("dictation.holdBackLabel", "Hold-back before sending")}
				value={dictationStore.state.handsFreeHoldBackMs}
				onChange={(v) => dictationStore.setHandsFreeHoldBackMs(v)}
				min={0}
				max={5000}
				step={250}
				formatValue={(v) => (v === 0 ? t("dictation.instant", "Instant") : `${v}ms`)}
				hint={t(
					"dictation.holdBackHint",
					"How long a finished utterance is shown before it is sent, so you can stop one you did not mean. Applies to the next conversation, not the one already running.",
				)}
			/>
		</div>
	);
};
