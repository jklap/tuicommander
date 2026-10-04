import { type Component, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { t } from "../../../i18n";
import { invoke } from "../../../invoke";
import { HttpRpcError } from "../../../transport";
import { SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";
import telegram from "./TelegramTab.module.css";

interface TelegramSettings {
	enabled: boolean;
	token_set: boolean;
	bot_alias: string;
	registered_agent_name: string | null;
	chats: string[];
	connected: boolean;
	last_error: string | null;
	last_message_time: number | null;
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
			if (!disposed) setSettings(result);
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
		<div class={`${s.section} ${telegram.panel}`}>
			<h3>Telegram</h3>
			<p class={s.hint}>
				Configure this machine. Polling runs only in tuic-remote; desktop Settings never starts another bot owner.
			</p>
			<Show when={settings()}>
				{(data) => (
					<>
						<div class={s.group}>
							<label for="telegram-token">{t("telegram.token", "Bot token")}</label>
							<p class={s.hint}>{data().token_set ? "Token set — enter a replacement" : "Token not set"}</p>
							<input
								id="telegram-token"
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
							<p class={s.hint}>{data().bot_alias}</p>
						</div>
						<div class={s.group}>
							<label for="telegram-chat">{t("telegram.chat", "Authorized chats")}</label>
							<input
								id="telegram-chat"
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
							<For each={data().chats}>
								{(id) => (
									<p>
										{id}{" "}
										<button
											class={s.testBtn}
											disabled={busy()}
											onClick={() => void change({ action: "remove_chat", chat_id: id })}
										>
											Remove
										</button>
									</p>
								)}
							</For>
							<button
								class={s.testBtn}
								disabled={busy() || !data().enabled}
								onClick={() => void change({ action: "pair" })}
							>
								Link chat
							</button>
							<Show when={code()}>
								<p class={s.hint}>
									Send {code()} to the bot. One use, valid for 10 minutes. A bare /start does not authorize a chat.
								</p>
							</Show>
						</div>
						<p aria-live="polite">
							{data().registered_agent_name
								? `registered agent: ${data().registered_agent_name}`
								: "nessun agent registrato"}
						</p>
						<SettingToggle
							label={t("telegram.enabled", "Enable Telegram")}
							checked={data().enabled}
							onChange={(enabled) => {
								if (!busy()) void change({ action: "configure", enabled });
							}}
						/>
						<p role="status">
							{data().connected ? "Connected" : data().enabled ? "Waiting for daemon connection" : "Disabled"}
						</p>
						<Show when={data().last_error}>
							<p class={s.warning}>{data().last_error}</p>
						</Show>
						<Show when={data().last_message_time}>
							<p class={s.hint}>Last message: {new Date(data().last_message_time ?? 0).toLocaleString()}</p>
						</Show>
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
