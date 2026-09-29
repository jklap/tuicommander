import { Show } from "solid-js";
import styles from "./TopBar.module.css";

interface TopBarProps {
	notificationCount?: number;
	isConnected?: boolean;
	onNotificationsClick?: () => void;
}

export function TopBar(props: TopBarProps) {
	const connected = () => props.isConnected ?? true;
	return (
		<header class={styles.topBar}>
			<div class={styles.titleGroup}>
				<div class={styles.titleRow}>
					<span class={styles.appName}>TUICommander</span>
					<span
						class={styles.connDot}
						classList={{ [styles.connOnline]: connected(), [styles.connOffline]: !connected() }}
						title={connected() ? "Connected" : "Offline"}
					/>
				</div>
				<span class={styles.subtitle}>{connected() ? "Manage your sessions" : "Reconnecting\u2026"}</span>
			</div>
			<Show when={(props.notificationCount ?? 0) > 0}>
				<button
					type="button"
					class={styles.badge}
					aria-label={`Open ${props.notificationCount} waiting session${props.notificationCount === 1 ? "" : "s"}`}
					onClick={props.onNotificationsClick}
				>
					{props.notificationCount}
				</button>
			</Show>
		</header>
	);
}
