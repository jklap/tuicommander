import { createSignal, Show } from "solid-js";
import styles from "./TopBar.module.css";

interface TopBarProps {
	isConnected?: boolean;
	onOpenSettings?: () => void;
}

export function TopBar(props: TopBarProps) {
	const connected = () => props.isConnected ?? true;
	const [menuOpen, setMenuOpen] = createSignal(false);
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
			<Show when={props.onOpenSettings}>
				<button
					type="button"
					class={styles.more}
					aria-label="More options"
					aria-haspopup="menu"
					aria-expanded={menuOpen()}
					onClick={() => setMenuOpen(!menuOpen())}
				>
					<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
						<circle cx="12" cy="5" r="1.75" />
						<circle cx="12" cy="12" r="1.75" />
						<circle cx="12" cy="19" r="1.75" />
					</svg>
				</button>
				<Show when={menuOpen()}>
					<div class={styles.menu} role="menu" aria-label="More options">
						<button
							type="button"
							role="menuitem"
							class={styles.menuItem}
							onClick={() => {
								setMenuOpen(false);
								props.onOpenSettings?.();
							}}
						>
							Settings
						</button>
					</div>
				</Show>
			</Show>
		</header>
	);
}
