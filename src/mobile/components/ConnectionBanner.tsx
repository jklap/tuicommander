import { Match, Switch } from "solid-js";
import styles from "./ConnectionBanner.module.css";

interface ConnectionBannerProps {
	/** Last poll failed, or the version check lost the server. */
	offline: boolean;
	/** The server refused this client (401/403). */
	authError: boolean;
	onRetry: () => void;
}

/** One strip for every way the phone can lose the server; the screens below keep their last data. */
export function ConnectionBanner(props: ConnectionBannerProps) {
	return (
		<Switch>
			<Match when={props.authError}>
				<div class={styles.banner} role="alert">
					<span>Access refused — sign in again</span>
					<a class={styles.action} href={`/mobile/login?next=${encodeURIComponent(location.pathname)}`}>
						Sign in
					</a>
				</div>
			</Match>
			<Match when={props.offline}>
				<div class={styles.banner} role="status">
					<span>Server unreachable — showing last known data, retrying…</span>
					<button type="button" class={styles.action} onClick={props.onRetry}>
						Retry now
					</button>
				</div>
			</Match>
		</Switch>
	);
}
