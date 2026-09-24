import { type Component, Show } from "solid-js";
import { AVAILABLE_LOCALES, localeName, t } from "../../../i18n";
import { appLogger } from "../../../stores/appLogger";
import type { UpdateChannel } from "../../../stores/settings";
import { settingsStore } from "../../../stores/settings";
import { updaterStore } from "../../../stores/updater";
import { SettingSelect, SettingSlider, SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";

export const GeneralTab: Component = () => {
	const languageOptions = AVAILABLE_LOCALES.map((value) => ({ value, label: localeName(value) }));

	const updateChannelOptions = [
		{ value: "stable", label: t("general.channel.stable", "Stable") },
		{ value: "nightly", label: t("general.channel.nightly", "Nightly") },
	];

	return (
		<div class={s.section}>
			<h3>{t("general.heading.general", "General")}</h3>

			{/* A picker with one option is not a choice. */}
			<Show when={AVAILABLE_LOCALES.length > 1}>
				<SettingSelect
					label={t("general.label.language", "Language")}
					value={settingsStore.state.language}
					onChange={(v) => settingsStore.setLanguage(v)}
					options={languageOptions}
					hint={t("general.hint.language", "Language of the TUICommander interface")}
				/>
			</Show>

			<SettingToggle
				checked={settingsStore.state.showLastPrompt}
				onChange={(v) => settingsStore.setShowLastPrompt(v)}
				label="Show agent context bar"
				hint="Display the model's current intent, its orchestrator-assigned task, and the last prompt sent to an agent"
			/>

			<h3>{t("general.heading.confirmations", "Confirmations")}</h3>

			<SettingToggle
				checked={settingsStore.state.confirmBeforeQuit}
				onChange={(v) => settingsStore.setConfirmBeforeQuit(v)}
				label={t("general.toggle.confirmBeforeQuit", "Confirm before quitting")}
				hint={t("general.hint.confirmBeforeQuit", "Show a confirmation dialog when closing the app")}
			/>

			<SettingToggle
				checked={settingsStore.state.confirmBeforeClosingTab}
				onChange={(v) => settingsStore.setConfirmBeforeClosingTab(v)}
				label={t("general.toggle.confirmBeforeClosingTab", "Confirm before closing a tab")}
				hint={t("general.hint.confirmBeforeClosingTab", "Show a confirmation dialog when closing a terminal tab")}
			/>

			<h3>{t("general.heading.powerManagement", "Power Management")}</h3>

			<SettingToggle
				checked={settingsStore.state.preventSleepWhenBusy}
				onChange={(v) => settingsStore.setPreventSleepWhenBusy(v)}
				label={t("general.toggle.preventSleepWhenBusy", "Prevent sleep when busy")}
				hint={t("general.hint.preventSleepWhenBusy", "Keep the system awake while scripts are running")}
			/>

			<SettingSlider
				label="Auto-Standby Timeout"
				value={settingsStore.state.standbyTimeoutMinutes}
				onChange={(v) => settingsStore.setStandbyTimeoutMinutes(v)}
				min={0}
				max={60}
				step={1}
				formatValue={(v) => (v === 0 ? "Off" : `${v} min`)}
				hint="Pause idle background sessions after this duration to save resources. 0 = disabled."
			/>

			<SettingSelect
				label="Content Indexing"
				value={settingsStore.state.indexStrategy}
				onChange={(v) =>
					settingsStore.setIndexStrategy(v as "active_only" | "active_and_switch" | "all_sequential" | "disabled")
				}
				options={[
					{ value: "disabled", label: "Disabled" },
					{ value: "active_only", label: "Active repo only" },
					{ value: "active_and_switch", label: "Active + on switch" },
					{ value: "all_sequential", label: "All repos at boot" },
				]}
				hint="When to build search indexes. Set to Disabled to turn off background indexing entirely."
			/>

			<h3>{t("general.heading.updates", "Updates")}</h3>

			<SettingToggle
				checked={settingsStore.state.autoUpdateEnabled}
				onChange={(v) => settingsStore.setAutoUpdateEnabled(v)}
				label={t("general.toggle.autoUpdateEnabled", "Automatically check for updates")}
				hint={t("general.hint.autoUpdateEnabled", "Download and install updates in the background")}
			/>

			<SettingSelect
				label={t("general.label.updateChannel", "Update Channel")}
				value={settingsStore.state.updateChannel}
				onChange={(v) => settingsStore.setUpdateChannel(v as UpdateChannel)}
				options={updateChannelOptions}
				hint={
					settingsStore.state.updateChannel !== "stable"
						? t("general.hint.updateChannelWarning", "Nightly builds may be unstable")
						: t("general.hint.updateChannel", "Choose which release channel to receive updates from")
				}
				hintStyle={settingsStore.state.updateChannel !== "stable" ? { color: "var(--warning, #e5c07b)" } : undefined}
			/>

			<div class={s.group}>
				<button
					class={s.testBtn}
					onClick={() => {
						updaterStore.checkForUpdate().catch((err: unknown) => appLogger.debug("app", "Update check failed", err));
					}}
					disabled={updaterStore.state.checking || updaterStore.state.downloading}
				>
					{updaterStore.state.checking
						? t("general.btn.checking", "Checking...")
						: t("general.btn.checkNow", "Check Now")}
				</button>
				<Show when={updaterStore.state.available && updaterStore.state.version}>
					<p class={s.hint} style={{ color: "var(--success)" }}>
						{t("general.hint.updateAvailable", "Version {version} is available!", {
							version: updaterStore.state.version ?? "",
						})}
					</p>
				</Show>
				<Show
					when={
						!updaterStore.state.available &&
						!updaterStore.state.checking &&
						!updaterStore.state.error &&
						!updaterStore.state.noRelease &&
						!updaterStore.state.unsupported
					}
				>
					<p class={s.hint}>{t("general.hint.latestVersion", "You are on the latest version")}</p>
				</Show>
				<Show when={updaterStore.state.unsupported}>
					<p class={s.hint} style={{ color: "var(--fg-muted)" }}>
						{updaterStore.state.unsupported}
					</p>
				</Show>
				<Show when={updaterStore.state.noRelease}>
					<p class={s.hint} style={{ color: "var(--fg-muted)" }}>
						{t("general.hint.noRelease", "No {channel} releases published yet", {
							channel: settingsStore.state.updateChannel,
						})}
					</p>
				</Show>
				<Show when={updaterStore.state.error}>
					<p class={s.hint} style={{ color: "var(--accent-red, #f44747)" }}>
						{updaterStore.state.error}
					</p>
				</Show>
			</div>

			<h3>{t("general.heading.experimental", "Experimental Features")}</h3>

			<div class={s.group}>
				<p class={s.warning}>
					{t("general.hint.experimentalWarning", "These features are under active development and may be unstable.")}
				</p>
				<div class={s.toggle}>
					<input
						type="checkbox"
						checked={settingsStore.state.experimentalFeaturesEnabled}
						onChange={(e) => settingsStore.setExperimentalFeaturesEnabled(e.currentTarget.checked)}
					/>
					<span>{t("general.toggle.experimentalFeatures", "Enable experimental features")}</span>
				</div>
				<p class={s.hint}>
					{t(
						"general.hint.experimentalFeatures",
						"Opt in to features under active development: the AI Chat panel and SSH Tunnels.",
					)}
				</p>
			</div>
		</div>
	);
};
