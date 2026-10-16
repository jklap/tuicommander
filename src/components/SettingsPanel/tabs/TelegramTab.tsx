import { type Component, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { t } from "../../../i18n";
import { invoke } from "../../../invoke";
import { HttpRpcError } from "../../../transport";
import { Input } from "../../ui/Input";
import { SettingStatus, SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";
import { EmptyState, ListRow, SectionCard, SettingPageHeader, SettingRow, StatusSummary } from "../SettingsPrimitives";

interface TelegramEvent {
	id: number;
	time: number;
	direction: "in" | "out";
	kind: string;
	outcome: string;
	preview: string;
}

interface TelegramSettings {
	enabled: boolean;
	token_set: boolean;
	bot_alias: string;
	registered_agent_name: string | null;
	chats: string[];
	pairing_pending: boolean;
	recent_events: TelegramEvent[];
	connected: boolean;
	polling_owner: "this_app" | "tuic_remote" | "desktop" | "another_process" | null;
	last_error: string | null;
	last_message_time: number | null;
}
function connectionLabel(data: TelegramSettings): string {
	if (!data.enabled) return t("telegram.disabled", "Disabled — enable Telegram to start polling.");
	if (!data.token_set) return t("telegram.missingToken", "No bot token — save and check a token first.");
	if (data.last_error)
		return t(
			"telegram.connectionError",
			"Polling stopped or retrying. Resolve the error below, then disable and enable Telegram.",
		);
	if (!data.connected)
		return t(
			"telegram.disconnected",
			"No poller connected. Check the bot token and enable Telegram in this app or tuic-remote.",
		);
	switch (data.polling_owner) {
		case "this_app":
			return t("telegram.connectedHere", "Connected (this app)");
		case "tuic_remote":
			return t("telegram.connectedRemote", "Connected (tuic-remote)");
		case "desktop":
			return t("telegram.connectedDesktop", "Connected (desktop app)");
		default:
			return t("telegram.connectedOther", "Connected (another process)");
	}
}

export const TelegramTab: Component = () => {
	const [settings, setSettings] = createSignal<TelegramSettings>();
	const [token, setToken] = createSignal("");
	const [chat, setChat] = createSignal("");
	const [code, setCode] = createSignal("");
	const [error, setError] = createSignal("");
	const [busy, setBusy] = createSignal(false);
	let disposed = false;
	let timer: ReturnType<typeof setInterval> | undefined;
	async function refresh() {
		try {
			const result = await invoke<TelegramSettings>("telegram_settings");
			if (!disposed) {
				setSettings(result);
				if (!result.pairing_pending) setCode("");
			}
		} catch {
			if (!disposed) setError("Telegram settings unavailable");
		}
	}
	async function change(value: Record<string, unknown>) {
		setBusy(true);
		setError("");
		try {
			const result = await invoke<{ code?: string }>("telegram_setup", { change: value });
			if (result.code) setCode(result.code);
			await refresh();
		} catch (error) {
			const detail = error instanceof HttpRpcError ? error.detail : typeof error === "string" ? error : "";
			setError(detail.startsWith("telegram_") ? detail : "Telegram setup failed. Check the token and chat ID.");
		} finally {
			setBusy(false);
		}
	}
	onMount(() => {
		void refresh();
		timer = setInterval(() => {
			if (!busy()) void refresh();
		}, 3000);
	});
	onCleanup(() => {
		disposed = true;
		clearInterval(timer);
	});
	return (
		<div class={s.section}>
			<SettingPageHeader
				title="Telegram"
				description="Bot setup and authorized chats on this machine."
				scope="This machine"
				breadcrumb="Integrations / Telegram"
				source={t(
					"telegram.pollingHint",
					"This app and tuic-remote share one polling owner on this machine. The other process stays on standby.",
				)}
			/>
			<Show when={settings()}>
				{(data) => (
					<>
						<StatusSummary
							label={connectionLabel(data())}
							tone={data().last_error || error() ? "error" : data().connected ? "success" : "warning"}
							source={
								error() ||
								data().last_error ||
								`Polling owner: ${{ this_app: "This app", tuic_remote: "tuic-remote", desktop: "Desktop app", another_process: "Another process", none: "Not connected" }[data().polling_owner ?? "none"] ?? "Not connected"} · Agent: ${data().registered_agent_name ?? "Not registered"}`
							}
							freshness={
								data().last_message_time
									? `Last message: ${new Date(data().last_message_time!).toLocaleString()}`
									: undefined
							}
						>
							<button class={s.testBtn} onClick={() => void refresh()} disabled={busy()}>
								Reload status
							</button>
						</StatusSummary>
						<SectionCard title="Bot setup">
							<SettingToggle
								row
								label={t("telegram.enabled", "Enable Telegram")}
								checked={data().enabled}
								onChange={(enabled) => {
									if (!busy()) void change({ action: "configure", enabled });
								}}
							/>
							<SettingRow
								label={t("telegram.token", "Bot token")}
								hint={
									data().token_set
										? `Token set${data().bot_alias ? ` (${data().bot_alias})` : ""}. Enter a replacement to change it.`
										: "Token not set"
								}
							>
								{(id, descriptionId) => (
									<div class={s.passwordRow}>
										<Input
											id={id}
											aria-describedby={descriptionId()}
											class={s.input}
											type="password"
											autocomplete="new-password"
											value={token()}
											onInput={(e) => setToken(e.currentTarget.value)}
										/>
										<button
											class={s.testBtn}
											disabled={busy() || !token()}
											onClick={() => {
												const value = token();
												setToken("");
												void change({ action: "token", token: value });
											}}
										>
											Save and check bot
										</button>
									</div>
								)}
							</SettingRow>
						</SectionCard>
						<SectionCard title="Chat access">
							<Show
								when={data().chats.length > 0}
								fallback={
									<EmptyState
										title="No authorized chats"
										description="Add a private chat ID or link a chat to allow messages from it."
									/>
								}
							>
								<For each={data().chats}>
									{(id) => (
										<ListRow
											name={id}
											actions={
												<button
													class={s.testBtn}
													disabled={busy()}
													onClick={() => void change({ action: "remove_chat", chat_id: id })}
												>
													Remove
												</button>
											}
										/>
									)}
								</For>
							</Show>
							<SettingRow label={t("telegram.chat", "Authorized chats")} hint="Private chat ID">
								{(id, descriptionId) => (
									<div class={s.passwordRow}>
										<Input
											id={id}
											aria-describedby={descriptionId()}
											class={s.input}
											inputmode="numeric"
											placeholder="Private chat ID"
											value={chat()}
											onInput={(e) => setChat(e.currentTarget.value)}
										/>
										<button
											class={s.testBtn}
											disabled={busy() || !chat()}
											onClick={() => void change({ action: "add_chat", chat_id: chat() })}
										>
											Add chat ID
										</button>
									</div>
								)}
							</SettingRow>
							<SettingRow
								label="Link a chat"
								hint={
									code() && data().enabled && data().connected
										? `Send ${code()} to the bot. One use, valid for 10 minutes. A bare /start does not authorize a chat.`
										: !data().connected
											? t(
													"telegram.pairingUnavailable",
													"Connect a poller before linking a chat. Check the connection status below.",
												)
											: "Generate a one-use code, then send it to the bot from your chat."
								}
							>
								{() => (
									<button
										class={s.testBtn}
										disabled={busy() || !data().enabled || !data().connected}
										onClick={() => void change({ action: "pair" })}
									>
										Link chat
									</button>
								)}
							</SettingRow>
						</SectionCard>
						<SectionCard
							title="Agent registration"
							description={t(
								"telegram.registrationHint",
								"Ask an agent in the polling app to call the Telegram MCP tool with action register. Registration lasts until the agent or polling app exits.",
							)}
						>
							<SettingRow label="Registered agent">
								{() => (
									<span aria-live="polite">
										{data().registered_agent_name
											? `registered agent: ${data().registered_agent_name}`
											: t("telegram.noAgent", "No agent registered")}
									</span>
								)}
							</SettingRow>
						</SectionCard>
						<SectionCard
							title="Recent messages"
							description="Last 20 events in this process, newest first. Cleared on restart. Credentials and unauthorized text are redacted."
						>
							<Show when={data().recent_events?.length} fallback={<p class={s.hint}>No recent Telegram events.</p>}>
								<For each={data().recent_events}>
									{(event) => (
										<SettingStatus
											label={`${new Date(event.time).toLocaleString()} · ${event.direction} · ${event.kind} · ${event.outcome}`}
											value={event.preview}
										/>
									)}
								</For>
							</Show>
						</SectionCard>
					</>
				)}
			</Show>
			<Show when={error()}>
				<p role="alert" class={s.warning}>
					{error()}
				</p>
			</Show>
		</div>
	);
};
