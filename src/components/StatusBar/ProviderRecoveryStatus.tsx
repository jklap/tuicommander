import { type Component, createSignal, onCleanup } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import type { ProviderRecovery } from "../../stores/terminals";
import { rpc } from "../../transport";
import { sendCommand } from "../../utils/sendCommand";
import s from "./ProviderRecoveryStatus.module.css";

export function recoveryLabel(recovery: ProviderRecovery, now: number): string {
	const next = recovery.next_attempt_ms;
	return `Waiting for provider recovery · ${recovery.phase} · attempt ${recovery.attempt}${next === null ? "" : ` · next ${new Date(next).toLocaleTimeString()} (${Math.max(0, Math.ceil((next - now) / 1000))}s)`}`;
}

export const ProviderRecoveryStatus: Component<{ recovery: ProviderRecovery; sessionId: string }> = (props) => {
	const [now, setNow] = createSignal(Date.now());
	const [sending, setSending] = createSignal(false);
	const timer = setInterval(() => setNow(Date.now()), 1000);
	onCleanup(() => clearInterval(timer));
	const retry = async () => {
		if (sending()) return;
		setSending(true);
		const sessionId = props.sessionId;
		try {
			await sendCommand((data) => rpc("write_pty", { sessionId, data }), "/retry", "ego");
		} catch (error) {
			appLogger.warn("terminal", "Force retry failed", error);
		} finally {
			setSending(false);
		}
	};
	return (
		<span role="status" class={s.recovery}>
			<span class={s.label} title={recoveryLabel(props.recovery, now())}>
				{recoveryLabel(props.recovery, now())}
			</span>
			<button type="button" class={s.retry} disabled={sending()} onClick={retry}>
				Force retry
			</button>
		</span>
	);
};
