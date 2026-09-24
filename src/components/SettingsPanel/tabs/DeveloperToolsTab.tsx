import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { t } from "../../../i18n";
import { invoke } from "../../../invoke";
import { appLogger } from "../../../stores/appLogger";
import type { CustomLauncher, IdeType } from "../../../stores/settings";
import { IDE_NAMES, settingsStore } from "../../../stores/settings";
import { isTauri } from "../../../transport";
import { SettingSelect } from "../SettingFields";
import s from "../Settings.module.css";

interface CliStatus {
	installed: boolean;
	path: string | null;
	version_match: boolean;
	auto_updatable: boolean;
	prompt_dismissed: boolean;
}

interface MdkbStatus {
	available: boolean;
	connected: boolean;
	binaryPath: string | null;
	version: string | null;
}

export const DeveloperToolsTab: Component = () => {
	const [cliStatus, setCliStatus] = createSignal<CliStatus | null>(null);
	const [cliInstalling, setCliInstalling] = createSignal(false);
	const [mdkbStatus, setMdkbStatus] = createSignal<MdkbStatus | null>(null);
	const [mdkbInstalling, setMdkbInstalling] = createSignal(false);
	const [mdkbError, setMdkbError] = createSignal<string | null>(null);

	const refreshCliStatus = async () => {
		if (!isTauri()) return;
		try {
			const status = await invoke<CliStatus>("get_cli_status");
			setCliStatus(status);
		} catch (err) {
			appLogger.error("app", "Failed to get CLI status", err);
		}
	};

	const refreshMdkbStatus = async () => {
		if (!isTauri()) return;
		try {
			const status = await invoke<MdkbStatus>("mdkb_status");
			setMdkbStatus(status);
		} catch (err) {
			appLogger.error("app", "Failed to get mdkb status", err);
		}
	};

	onMount(() => {
		refreshCliStatus();
		refreshMdkbStatus();
	});

	const handleInstallCli = async () => {
		setCliInstalling(true);
		try {
			await invoke<string>("install_cli");
			await refreshCliStatus();
		} catch (err) {
			appLogger.error("app", "Failed to install CLI", err);
		} finally {
			setCliInstalling(false);
		}
	};

	const handleUninstallCli = async () => {
		try {
			await invoke("uninstall_cli");
			await refreshCliStatus();
		} catch (err) {
			appLogger.error("app", "Failed to uninstall CLI", err);
		}
	};

	const handleInstallMdkb = async () => {
		setMdkbInstalling(true);
		try {
			await invoke<string>("install_mdkb");
			await refreshMdkbStatus();
		} catch (err) {
			appLogger.error("app", "Failed to install mdkb", err);
		} finally {
			setMdkbInstalling(false);
		}
	};

	const handleUninstallMdkb = async () => {
		setMdkbError(null);
		try {
			await invoke("uninstall_mdkb");
			await refreshMdkbStatus();
		} catch (err) {
			const msg = typeof err === "string" ? err : String(err);
			setMdkbError(msg);
			appLogger.error("app", "Failed to uninstall mdkb", err);
		}
	};

	const ideOptions = Object.entries(IDE_NAMES).map(([value, label]) => ({ value, label }));

	// --- Custom launchers (GH #71) ---
	const launchers = (): CustomLauncher[] => settingsStore.state.customLaunchers;
	const updateLauncher = (id: string, patch: Partial<CustomLauncher>) =>
		settingsStore.setCustomLaunchers(launchers().map((l) => (l.id === id ? { ...l, ...patch } : l)));
	const addLauncher = () =>
		settingsStore.setCustomLaunchers([
			...launchers(),
			{ id: crypto.randomUUID(), name: "New tool", executable: "", args: [], enabled: true },
		]);
	const removeLauncher = (id: string) => settingsStore.setCustomLaunchers(launchers().filter((l) => l.id !== id));

	return (
		<div class={s.section}>
			<Show when={isTauri() && cliStatus()}>
				<h3>
					{t("general.heading.cli", "TUIC CLI")}
					<span class={s.infoBadge}>
						?
						<span class={s.infoBadgeTip}>
							{t(
								"general.hint.cliInfo",
								"The TUIC CLI lets you control TUICommander from any terminal or script. Open files and URLs as tabs, manage PTY sessions (create, list, send input, read output), and query repository status. Useful for scripting automation.",
							)}
						</span>
					</span>
				</h3>

				<div class={s.group}>
					<Show
						when={cliStatus()!.installed}
						fallback={
							<>
								<p class={s.hint}>
									{t(
										"general.hint.cliNotInstalled",
										"Install the TUIC CLI to control TUICommander from any terminal or script. Open files and URLs as tabs, manage PTY sessions, and query repository status. Useful for scripting automation.",
									)}
								</p>
								<button
									class={s.testBtn}
									onClick={handleInstallCli}
									disabled={cliInstalling()}
									style={{ "margin-top": "8px" }}
								>
									{cliInstalling()
										? t("general.btn.installing", "Installing...")
										: t("general.btn.installCli", "Install TUIC CLI")}
								</button>
							</>
						}
					>
						<p class={s.hint} style={{ color: "var(--success)" }}>
							{t("general.hint.cliInstalled", "Installed at {path}", {
								path: cliStatus()!.path ?? "/usr/local/bin/tuic",
							})}
							{!cliStatus()!.version_match && (
								<span style={{ color: "var(--warning, #e5c07b)", "margin-left": "8px" }}>
									{cliStatus()!.auto_updatable
										? t("general.hint.cliOutdated", "(update pending — restart to apply)")
										: t("general.hint.cliUpdateAvailable", "(update available)")}
								</span>
							)}
						</p>
						<div style={{ display: "flex", gap: "8px", "margin-top": "8px" }}>
							<Show when={!cliStatus()!.version_match}>
								<button class={s.testBtn} onClick={handleInstallCli} disabled={cliInstalling()}>
									{cliInstalling() ? t("general.btn.updating", "Updating...") : t("general.btn.updateCli", "Update")}
								</button>
							</Show>
							<button class={s.testBtn} onClick={handleUninstallCli}>
								{t("general.btn.uninstallCli", "Uninstall")}
							</button>
						</div>
					</Show>
				</div>
			</Show>

			<Show when={isTauri()}>
				<h3>
					{t("general.heading.codeIntelligence", "Code Intelligence")}
					<span class={s.infoBadge}>
						?
						<span class={s.infoBadgeTip}>
							{t(
								"general.hint.codeIntelligenceInfo",
								"Integrates with MDKB to provide code navigation features in the editor: Cmd+Click go-to-definition, Shift+F12 find references, and symbol outline. Also serves as a persistent memory manager for AI agents, fully integrated with TUICommander. MDKB indexes your repositories and exposes a local daemon for fast lookups.",
							)}
						</span>
					</span>
				</h3>

				<div class={s.group}>
					<Show
						when={mdkbStatus()?.available}
						fallback={
							<>
								<p class={s.hint}>
									{t(
										"general.hint.mdkbNotInstalled",
										"Install MDKB to enable outline, go-to-definition, and find references in the code editor.",
									)}
								</p>
								<button
									class={s.testBtn}
									onClick={handleInstallMdkb}
									disabled={mdkbInstalling()}
									style={{ "margin-top": "8px" }}
								>
									{mdkbInstalling()
										? t("general.btn.installing", "Installing...")
										: t("general.btn.installMdkb", "Install MDKB")}
								</button>
							</>
						}
					>
						<p class={s.hint} style={{ color: "var(--success)" }}>
							{t("general.hint.mdkbInstalled", "Installed at {path}", {
								path: mdkbStatus()!.binaryPath ?? "unknown",
							})}
							{mdkbStatus()!.version && (
								<span style={{ "margin-left": "8px", color: "var(--fg-muted)" }}>v{mdkbStatus()!.version}</span>
							)}
						</p>
						<button class={s.testBtn} onClick={handleUninstallMdkb} style={{ "margin-top": "8px" }}>
							{t("general.btn.uninstallMdkb", "Uninstall")}
						</button>
						<Show when={mdkbError()}>
							<p class={s.hint} style={{ color: "var(--error)" }}>
								{mdkbError()}
							</p>
						</Show>
					</Show>
				</div>
			</Show>

			<h3>{t("developerTools.heading.ide", "IDE")}</h3>

			<SettingSelect
				label={t("general.label.defaultIde", "Default IDE")}
				value={settingsStore.state.ide}
				onChange={(v) => settingsStore.setIde(v as IdeType)}
				options={ideOptions}
				hint={t("general.hint.defaultIde", "IDE used to open repositories")}
			/>

			<Show when={isTauri()}>
				<h3>{t("general.heading.customLaunchers", "Custom Launchers")}</h3>
				<div class={s.group}>
					<p class={s.hint}>
						{t(
							"general.hint.customLaunchers",
							'Define your own tools for the "Open in" menu. Each argument may use {path}, {file}, {fileDir}, {repo}, {cwd}, {home}, {line}, {column} placeholders. One argument per line.',
						)}
					</p>
					<For each={launchers()}>
						{(launcher) => (
							<div
								class={s.group}
								style={{
									border: "1px solid var(--border)",
									"border-radius": "var(--radius-md)",
									padding: "8px",
									"margin-bottom": "8px",
								}}
							>
								<div style={{ display: "flex", gap: "8px", "align-items": "center", "margin-bottom": "6px" }}>
									<input
										type="checkbox"
										checked={launcher.enabled}
										title={t("general.label.launcherEnabled", "Enabled")}
										onChange={(e) => updateLauncher(launcher.id, { enabled: e.currentTarget.checked })}
									/>
									<input
										type="text"
										value={launcher.name}
										placeholder={t("general.placeholder.launcherName", "Name")}
										onInput={(e) => updateLauncher(launcher.id, { name: e.currentTarget.value })}
										style={{ flex: "1" }}
									/>
									<button class={s.testBtn} onClick={() => removeLauncher(launcher.id)}>
										{t("general.btn.remove", "Remove")}
									</button>
								</div>
								<input
									type="text"
									value={launcher.executable}
									placeholder={t("general.placeholder.launcherExec", "Executable (e.g. code, or /usr/local/bin/code)")}
									onInput={(e) => updateLauncher(launcher.id, { executable: e.currentTarget.value })}
									style={{ width: "100%", "margin-bottom": "6px" }}
								/>
								<textarea
									value={launcher.args.join("\n")}
									placeholder={"--goto\n{file}:{line}:{column}"}
									rows={3}
									onInput={(e) => updateLauncher(launcher.id, { args: e.currentTarget.value.split("\n") })}
									style={{ width: "100%", "font-family": "var(--font-mono)", "font-size": "var(--font-sm)" }}
								/>
							</div>
						)}
					</For>
					<button class={s.testBtn} onClick={addLauncher}>
						{t("general.btn.addLauncher", "Add launcher")}
					</button>
				</div>
			</Show>
		</div>
	);
};
