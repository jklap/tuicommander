import { type Component, createSignal, For, Show } from "solid-js";
import { t } from "../../../i18n";
import { invoke } from "../../../invoke";
import type { NotificationSound, SoundPreset } from "../../../notifications";
import { tasksNotifications } from "../../../services/tasksNotifications";
import { appLogger } from "../../../stores/appLogger";
import { notificationsStore } from "../../../stores/notifications";
import { isTauri } from "../../../transport";
import { openDialog } from "../../../utils/nativeDialog";
import { pathBasename } from "../../../utils/pathUtils";
import { Select } from "../../ui/Select";
import { ContextualHelp } from "../ContextualHelp";
import { ExpertSetting } from "../ExpertSetting";
import { SettingSlider, SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";
import { SectionCard, SettingPageHeader, SettingRow } from "../SettingsPrimitives";

interface AudioOutputDevice {
	name: string;
	is_default: boolean;
}

/** Custom sound files are decoded natively by rodio (see
 *  src-tauri/src/notification_sound.rs) — these are the formats its enabled
 *  Cargo features actually support. */
const CUSTOM_SOUND_EXTENSIONS = ["wav", "mp3", "ogg", "flac"];

/** Display name for each sound's tone when offered as another event's preset
 *  — deliberately NOT just "<Event> tone", so it reads as a real alternative
 *  rather than "sounds like the X event". */
const PRESET_LABELS: Record<NotificationSound, string> = {
	question: "Chime",
	completion: "Arpeggio",
	error: "Low Tone",
	warning: "Double-Tap",
	info: "Pluck",
	attention: "Callback",
};

// ---------------------------------------------------------------------------
// Sound pattern visualizations (inline SVG showing pitch contour)
// ---------------------------------------------------------------------------

/** Mini musical staff showing the note pattern for each sound.
 *  5 staff lines, note heads positioned by pitch, stems going up. */
function SoundPatternSvg(props: { sound: NotificationSound }) {
	const patterns: Record<NotificationSound, { x: number; y: number }[]> = {
		question: [
			{ x: 14, y: 16 },
			{ x: 30, y: 8 },
		],
		completion: [
			{ x: 10, y: 16 },
			{ x: 24, y: 10 },
			{ x: 38, y: 4 },
		],
		error: [
			{ x: 14, y: 8 },
			{ x: 30, y: 16 },
		],
		warning: [
			{ x: 14, y: 12 },
			{ x: 30, y: 12 },
		],
		info: [{ x: 22, y: 4 }],
		// Two quick low knocks followed by a higher callback.
		attention: [
			{ x: 8, y: 16 },
			{ x: 22, y: 16 },
			{ x: 36, y: 4 },
		],
	};

	const colors: Record<NotificationSound, string> = {
		question: "var(--warning)",
		completion: "var(--success)",
		error: "var(--error)",
		warning: "var(--accent)",
		info: "var(--fg-muted)",
		attention: "var(--error)",
	};

	const notes = patterns[props.sound];
	const color = colors[props.sound];
	const w = props.sound === "completion" || props.sound === "attention" ? 36 : props.sound === "info" ? 24 : 32;

	return (
		<svg viewBox={`0 0 ${w} 18`} width={w} height="14" style={{ "vertical-align": "middle", "flex-shrink": "0" }}>
			<For each={[3, 6, 9, 12, 15]}>
				{(ly) => <line x1="1" y1={ly} x2={w - 1} y2={ly} stroke="var(--border)" stroke-width="0.4" />}
			</For>
			<For each={notes}>
				{(note) => {
					const sy = (note.y / 20) * 15;
					const sx = (note.x / 48) * w;
					return (
						<>
							<ellipse cx={sx} cy={sy} rx="2.5" ry="1.8" fill={color} transform={`rotate(-15 ${sx} ${sy})`} />
							<line x1={sx + 2.3} y1={sy - 0.5} x2={sx + 2.3} y2={sy - 7} stroke={color} stroke-width="0.7" />
						</>
					);
				}}
			</For>
		</svg>
	);
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const NotificationsTab: Component = () => {
	const sounds: { key: NotificationSound; label: string }[] = [
		{ key: "question", label: t("notifications.sound.question", "Question") },
		{ key: "error", label: t("notifications.sound.error", "Error") },
		{ key: "completion", label: t("notifications.sound.completion", "Completion") },
		{ key: "warning", label: t("notifications.sound.warning", "Warning") },
		{ key: "info", label: t("notifications.sound.info", "Info") },
		{ key: "attention", label: t("notifications.sound.attention", "Attention (agent needs you)") },
	];

	// Device enumeration is LAZY: on macOS, cpal's CoreAudio output-device scan
	// triggers the microphone permission prompt. We must NOT run it on tab mount —
	// only when the user explicitly clicks "Choose output device". `null` = not loaded yet.
	const [devices, setDevices] = createSignal<AudioOutputDevice[] | null>(null);
	const [loadingDevices, setLoadingDevices] = createSignal(false);

	async function loadDevices(): Promise<void> {
		if (!isTauri()) return;
		setLoadingDevices(true);
		try {
			// The scan can wait on the microphone prompt until the backend times
			// out. A failure keeps the previous list (never "no devices") and one
			// more try picks up the answer that arrived late.
			for (let attempt = 0; attempt < 2; attempt++) {
				try {
					setDevices(await invoke<AudioOutputDevice[]>("list_audio_output_devices"));
					return;
				} catch (err) {
					appLogger.warn("settings", "Failed to enumerate audio output devices", err);
				}
			}
		} finally {
			setLoadingDevices(false);
		}
	}

	return (
		<div class={`${s.section} ${s.applicationPage}`}>
			<SettingPageHeader title="Notifications" breadcrumb="Application / Notifications" scope="Application defaults" />
			<SectionCard title={t("notifications.heading.notificationSettings", "Notification Settings")}>
				<Show
					when={notificationsStore.state.isAvailable}
					fallback={
						<p class={s.warning}>
							{t("notifications.warning.notAvailable", "Audio notifications are not available on this platform")}
						</p>
					}
				>
					<SettingToggle
						settingId="notifications.enabled"
						row
						checked={notificationsStore.state.config.enabled}
						onChange={(v) => notificationsStore.setEnabled(v)}
						label={t("notifications.toggle.enableAudio", "Enable audio notifications")}
					/>

					<ExpertSetting configKey="notifications.volume" value={notificationsStore.state.config.volume}>
						<SettingSlider
							settingId="notifications.volume"
							row
							label={t("notifications.label.masterVolume", "Master Volume")}
							value={Math.round(notificationsStore.state.config.volume * 100)}
							onChange={(v) => notificationsStore.setVolume(v / 100)}
							onCommit={() => notificationsStore.testSound("info")}
							min={0}
							max={100}
							suffix="%"
							hint={t("notifications.hint.masterVolume", "Overall volume for all notification sounds")}
						/>
					</ExpertSetting>

					<Show when={isTauri()}>
						<ExpertSetting
							configKey="notifications.audio_device"
							value={notificationsStore.state.config.audio_device ?? null}
						>
							<SettingRow label={t("notifications.label.audioDevice", "Audio Output Device")}>
								{(id, descriptionId) => (
									<div>
										<Show
											when={devices() !== null}
											fallback={
												<>
													<p class={s.hint}>
														{t("notifications.hint.audioDeviceCurrent", "Currently: {device}", {
															device:
																notificationsStore.state.config.audio_device ??
																t("notifications.option.systemDefault", "System Default"),
														})}
													</p>
													<button
														data-non-setting="Explicit output-device discovery; does not select a device"
														id={id}
														aria-label={t("notifications.btn.chooseDevice", "Choose output device…")}
														aria-describedby={descriptionId()}
														class={s.testBtn}
														disabled={loadingDevices()}
														onClick={loadDevices}
													>
														{loadingDevices()
															? t("notifications.btn.loadingDevices", "Loading…")
															: t("notifications.btn.chooseDevice", "Choose output device…")}
													</button>
												</>
											}
										>
											<Select
												data-setting-id="notifications.audio_device"
												id={id}
												aria-describedby={descriptionId()}
												value={notificationsStore.state.config.audio_device ?? ""}
												onChange={(e) => {
													const val = e.currentTarget.value;
													notificationsStore.setAudioDevice(val || null);
												}}
											>
												<option value="">{t("notifications.option.systemDefault", "System Default")}</option>
												<For each={devices()}>
													{(device) => (
														<option value={device.name}>
															{device.name}
															{device.is_default
																? ` (${t("notifications.option.currentDefault", "current default")})`
																: ""}
														</option>
													)}
												</For>
											</Select>
											<button
												data-non-setting="Explicit output-device discovery; does not select a device"
												class={s.testBtn}
												style={{ "margin-top": "6px" }}
												disabled={loadingDevices()}
												onClick={loadDevices}
											>
												{t("notifications.btn.refreshDevices", "Refresh")}
											</button>
										</Show>
									</div>
								)}
							</SettingRow>
						</ExpertSetting>
					</Show>

					<div class={s.group}>
						<h4 class={s.notificationEventsHeading}>
							<label>{t("notifications.label.notificationEvents", "Notification Events")}</label>
						</h4>
						<For each={sounds}>
							{(sound) => {
								let selectRef: HTMLSelectElement | undefined;
								// A config without per-sound choices (older backend, partial fixture) means "default tone".
								const choice = () =>
									notificationsStore.state.config.sound_choices?.[sound.key] ?? {
										preset: "default",
										custom_path: null,
									};
								const otherSounds = sounds.filter((entry) => entry.key !== sound.key);

								async function handlePresetChange(e: Event & { currentTarget: HTMLSelectElement }): Promise<void> {
									const val = e.currentTarget.value as SoundPreset;
									if (val === "custom") {
										try {
											// main's crash-safe picker (native_dialog.rs), not the plugin's `open`.
											const picked = await openDialog({
												multiple: false,
												filters: [{ name: "Audio", extensions: CUSTOM_SOUND_EXTENSIONS }],
											});
											if (typeof picked === "string") {
												notificationsStore.setSoundChoice(sound.key, { preset: "custom", custom_path: picked });
												return;
											}
										} catch (err) {
											appLogger.warn("settings", "Failed to open the custom sound file picker", err);
										}
										// Canceled, or the picker itself failed — the browser already
										// flipped the <select>'s own displayed value to "custom"; force
										// it back since the store (and the reactive `value` binding)
										// never changed.
										if (selectRef) selectRef.value = choice().preset;
									} else {
										notificationsStore.setSoundChoice(sound.key, { preset: val, custom_path: null });
									}
								}

								return (
									<SettingRow label={sound.label}>
										{(id, descriptionId) => (
											<>
												<div class={s.soundControls}>
													<input
														id={id}
														aria-describedby={descriptionId()}
														type="checkbox"
														role="switch"
														class={s.rowSwitch}
														data-setting-id={`notifications.sounds.${sound.key}`}
														checked={notificationsStore.state.config.sounds[sound.key]}
														onChange={(event) =>
															notificationsStore.setSoundEnabled(sound.key, event.currentTarget.checked)
														}
													/>
													<span class={s.soundPattern}>
														<SoundPatternSvg sound={sound.key} />
													</span>
													<Select
														compact
														data-setting-id={`notifications.sound_choices.${sound.key}.preset`}
														aria-label={`${sound.label} sound source`}
														ref={(el) => {
															selectRef = el;
														}}
														value={choice().preset}
														onChange={handlePresetChange}
													>
														<option value="default">{t("notifications.preset.default", "Default")}</option>
														<For each={otherSounds}>
															{(other) => <option value={other.key}>{PRESET_LABELS[other.key]}</option>}
														</For>
														<Show
															when={isTauri()}
															fallback={
																// Desktop and a browser-mode dev instance share config.json (see
																// AGENTS.md's isolation caveat), so a custom choice made on desktop
																// can still be the current value here even though this mode can't
																// set or use one. Render it as a disabled option so the <select>
																// always has a matching value instead of silently selecting nothing.
																<Show when={choice().preset === "custom"}>
																	<option value="custom" disabled>
																		{t("notifications.preset.customDesktopOnly", "Custom file (desktop only)")}
																	</option>
																</Show>
															}
														>
															<option value="custom">{t("notifications.preset.custom", "Custom file…")}</option>
														</Show>
													</Select>
													<button
														data-non-setting="Preview the sound without changing its configuration"
														class={s.testBtn}
														aria-label={`Preview ${sound.label}`}
														onClick={() => notificationsStore.testSound(sound.key)}
													>
														{t("notifications.btn.test", "Test")}
													</button>
												</div>
												<Show when={choice().preset === "custom" && choice().custom_path}>
													<p class={`${s.hint} ${s.soundCustomFile}`}>
														{t("notifications.hint.customSound", "Custom: {file}", {
															file: pathBasename(choice().custom_path!),
														})}{" "}
														<button
															data-setting-id={`notifications.sound_choices.${sound.key}.preset`}
															class={s.testBtn}
															onClick={() =>
																notificationsStore.setSoundChoice(sound.key, { preset: "default", custom_path: null })
															}
														>
															{t("notifications.btn.resetSound", "Reset to default")}
														</button>
													</p>
												</Show>
											</>
										)}
									</SettingRow>
								);
							}}
						</For>
					</div>

					<div class={s.group}>
						<div class={s.helpLabel}>
							<label>{t("notifications.label.orchestration", "Orchestration")}</label>
							<ContextualHelp label={t("notifications.label.orchestration", "Orchestration")} />
						</div>
						<SettingToggle
							settingId="notifications.silence_remote_completions"
							row
							checked={notificationsStore.state.config.silence_remote_completions}
							onChange={(value) => notificationsStore.setSilenceRemoteCompletions(value)}
							label={t("notifications.label.silenceRemoteCompletions", "Silence completions from MCP sessions")}
						/>
						<p class={s.hint} style={{ "margin-top": "6px" }}>
							{t(
								"notifications.hint.silenceRemoteCompletions",
								"Sessions started by an agent orchestrator (session create, agent spawn) finish without a chime. They still appear in Activity and update the badge.",
							)}
						</p>
					</div>

					<div class={s.actions}>
						<button
							data-non-setting="Reset the notification owner document to its defaults"
							onClick={() => notificationsStore.reset()}
						>
							{t("notifications.btn.resetDefaults", "Reset Defaults")}
						</button>
					</div>
				</Show>
			</SectionCard>
			<SectionCard title="Visual notifications">
				{/* Outside the audio Show on purpose — the bell is visual, so the setting
			    must stay reachable on a machine with no audio output. */}
				<div class={s.group}>
					<div class={s.helpLabel}>
						<label>{t("notifications.label.toolbarBell", "Toolbar Bell")}</label>
						<ContextualHelp label={t("notifications.label.toolbarBell", "Toolbar Bell")} />
					</div>
					<SettingToggle
						settingId="notifications.toasts_in_bell"
						row
						checked={notificationsStore.state.config.toasts_in_bell}
						onChange={(value) => notificationsStore.setToastsInBell(value)}
						label={t("notifications.toggle.toastsInBell", "Keep toasts in the bell")}
					/>
					<p class={s.hint} style={{ "margin-top": "6px" }}>
						{t(
							"notifications.hint.toastsInBell",
							"Toasts fade on their own, often while you look at another window. Mirroring them into the bell keeps them readable afterwards. Turn this off to leave toasts transient.",
						)}
					</p>
				</div>

				<SettingToggle
					data-non-setting="Client-local toast-card preference; no backend owner"
					row
					checked={notificationsStore.state.autoToastCards}
					onChange={(enabled) => notificationsStore.setAutoToastCards(enabled)}
					label={t("notifications.toggle.autoToastCards", "Show cards from the active terminal")}
					hint={t(
						"notifications.hint.autoToastCards",
						"Show normal agent toast cards only on their active origin terminal tab. Other tabs receive bell entries only. Urgent notices always show a card. Saved for this client.",
					)}
				/>

				<div class={s.group}>
					<label>{t("notifications.label.prNotifications", "Pull Requests")}</label>
					<SettingToggle
						settingId="notifications.pr_native_notifications"
						row
						checked={notificationsStore.state.config.pr_native_notifications}
						onChange={(value) => notificationsStore.setPrNativeNotifications(value)}
						label={t("notifications.toggle.prNative", "OS notification for PR changes")}
					/>
					<p class={s.hint} style={{ "margin-top": "6px" }}>
						{t(
							"notifications.hint.prNative",
							"Notify when a PR becomes ready to merge, fails CI, gets changes requested or is merged, while TUICommander is in the background. Clicking the notification opens the PR on GitHub.",
						)}
					</p>
				</div>
			</SectionCard>
			<SectionCard title={t("notifications.heading.tasks", "Tasks")}>
				<SettingToggle
					data-non-setting="Client-local Tasks notification preference; no backend owner"
					row
					checked={tasksNotifications.enabled()}
					onChange={(enabled) => tasksNotifications.setEnabled(enabled)}
					label={t("notifications.toggle.tasks", "OS notifications for Tasks")}
					hint={t(
						"notifications.hint.tasks",
						"Off by default. Notify for failed checks on linked GitHub PRs and new comments on followed tasks during existing detail refreshes. No background item polling. Saved with Tasks preferences.",
					)}
				/>
			</SectionCard>
		</div>
	);
};
