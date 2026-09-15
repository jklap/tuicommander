import { type Component, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { appLogger } from "../../../stores/appLogger";
import { terminalsStore } from "../../../stores/terminals";
import { rpc } from "../../../transport";
import { updateAppConfig } from "../../../utils/updateAppConfig";
import { SettingSlider, SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";

interface StreamDockConfig {
	enabled: boolean;
	device_serial: string | null;
	screen_brightness: number;
	led_brightness: number;
	pinned_sessions: string[];
}

interface AppConfig {
	streamdock: StreamDockConfig;
}

interface StreamDockStatus {
	enabled: boolean;
	running: boolean;
	device: string | null;
	last_error: string | null;
	restarts: number;
}

interface StreamDockDeviceInfo {
	product_name: string;
	serial_number: string;
	vendor_id: number;
	product_id: number;
}

const DEFAULT_STREAMDOCK: StreamDockConfig = {
	enabled: false,
	device_serial: null,
	screen_brightness: 70,
	led_brightness: 40,
	pinned_sessions: [],
};

function statusColor(status: StreamDockStatus | null): string {
	if (!status) return "var(--text-secondary)";
	if (status.last_error) return "var(--error-color, #c43b3b)";
	if (status.running) return "var(--success-color, #2e9e5b)";
	return "var(--text-secondary)";
}

function statusLabel(status: StreamDockStatus | null): string {
	if (!status) return "Unknown";
	if (!status.enabled) return "Disabled";
	if (status.running && status.device) return `Connected — ${status.device}`;
	if (status.last_error) return status.last_error;
	return "Waiting for device…";
}

export const StreamDockTab: Component = () => {
	const [cfg, setCfg] = createSignal<StreamDockConfig>(DEFAULT_STREAMDOCK);
	const [status, setStatus] = createSignal<StreamDockStatus | null>(null);
	const [devices, setDevices] = createSignal<StreamDockDeviceInfo[]>([]);
	const [loadingDevices, setLoadingDevices] = createSignal(false);

	const saveConfigField = async (updater: (config: AppConfig) => void) => {
		try {
			const next = await updateAppConfig<AppConfig>(updater);
			setCfg(next.streamdock);
		} catch (e) {
			appLogger.error("config", "Failed to save StreamDock config", e);
		}
	};

	const loadConfig = async () => {
		try {
			const config = await rpc<AppConfig>("load_config");
			setCfg(config.streamdock ?? DEFAULT_STREAMDOCK);
		} catch (e) {
			appLogger.warn("config", "Failed to load StreamDock config, using defaults", e);
		}
	};

	const refreshStatus = async () => {
		try {
			setStatus(await rpc<StreamDockStatus>("streamdock_status"));
		} catch {
			// Status endpoint unavailable (non-desktop build) — leave status unknown.
		}
	};

	const refreshDevices = async () => {
		setLoadingDevices(true);
		try {
			setDevices(await rpc<StreamDockDeviceInfo[]>("streamdock_list_devices"));
		} catch (e) {
			appLogger.warn("streamdock", "Failed to list StreamDock devices", e);
		} finally {
			setLoadingDevices(false);
		}
	};

	onMount(() => {
		loadConfig();
		refreshStatus();
		refreshDevices();
		const interval = setInterval(refreshStatus, 2000);
		onCleanup(() => clearInterval(interval));
	});

	const liveSessions = () =>
		Object.values(terminalsStore.state.terminals).filter(
			(t): t is typeof t & { sessionId: string } => t.sessionId !== null,
		);

	const togglePinned = (sessionId: string, pinned: boolean) => {
		saveConfigField((c) => {
			const set = new Set(c.streamdock.pinned_sessions);
			if (pinned) set.add(sessionId);
			else set.delete(sessionId);
			c.streamdock.pinned_sessions = Array.from(set);
		});
	};

	return (
		<div class={s.section}>
			<h3>StreamDock M18</h3>
			<p class={s.hint}>
				A 15-key LCD macropad mirroring live session state — each key shows a session's status and focuses it on press.
				Desktop only.
			</p>

			<SettingToggle
				checked={cfg().enabled}
				onChange={(enabled) => saveConfigField((c) => (c.streamdock.enabled = enabled))}
				label="Enable StreamDock integration"
				hint="Attaches to the first connected StreamDock M18 (or the selected device below) and starts mirroring session state to its keys."
			/>

			<div class={s.group}>
				<label>StreamDock status</label>
				<div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
					<span
						style={{
							display: "inline-block",
							width: "10px",
							height: "10px",
							"border-radius": "50%",
							background: statusColor(status()),
							"flex-shrink": "0",
						}}
					/>
					<span>{statusLabel(status())}</span>
				</div>
				<Show when={status() && status()!.restarts > 0}>
					<p class={s.hint}>Reconnected {status()!.restarts} time(s) since enabled.</p>
				</Show>
			</div>

			<div class={s.group}>
				<label>Device</label>
				<select
					value={cfg().device_serial ?? ""}
					onChange={(e) => {
						const value = e.currentTarget.value || null;
						saveConfigField((c) => (c.streamdock.device_serial = value));
					}}
				>
					<option value="">First connected device</option>
					<For each={devices()}>
						{(d) => (
							<option value={d.serial_number}>
								{d.product_name} ({d.serial_number || `${d.vendor_id}:${d.product_id}`})
							</option>
						)}
					</For>
				</select>
				<div style={{ "margin-top": "8px" }}>
					<button type="button" class={s.testBtn} onClick={refreshDevices} disabled={loadingDevices()}>
						{loadingDevices() ? "Scanning…" : "Rescan devices"}
					</button>
				</div>
				<Show when={devices().length === 0 && !loadingDevices()}>
					<p class={s.hint}>No StreamDock device currently detected.</p>
				</Show>
			</div>

			<SettingSlider
				label="Screen brightness"
				value={cfg().screen_brightness}
				onChange={(v) => setCfg({ ...cfg(), screen_brightness: v })}
				onCommit={(v) => saveConfigField((c) => (c.streamdock.screen_brightness = v))}
				min={0}
				max={100}
				suffix="%"
			/>

			<SettingSlider
				label="LED brightness"
				value={cfg().led_brightness}
				onChange={(v) => setCfg({ ...cfg(), led_brightness: v })}
				onCommit={(v) => saveConfigField((c) => (c.streamdock.led_brightness = v))}
				min={0}
				max={100}
				suffix="%"
				hint="Only applies on firmware that reports RGB support (V3-class M18 units)."
			/>

			<h3>Pinned sessions</h3>
			<p class={s.hint}>
				A pinned session's key is never evicted to make room for another session, even one that needs input more
				urgently.
			</p>
			<Show when={liveSessions().length > 0} fallback={<p class={s.hint}>No live sessions to pin right now.</p>}>
				<For each={liveSessions()}>
					{(t) => (
						<SettingToggle
							checked={cfg().pinned_sessions.includes(t.sessionId)}
							onChange={(pinned) => togglePinned(t.sessionId, pinned)}
							label={t.alias ?? t.name ?? t.sessionId}
						/>
					)}
				</For>
			</Show>
		</div>
	);
};
