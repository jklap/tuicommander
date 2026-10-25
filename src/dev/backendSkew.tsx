import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import { Button } from "../components/ui/Button";
import styles from "./backendSkew.module.css";

interface BuildIdentity {
	fingerprint: string;
	git_hash: string;
}

interface HotUpdates {
	on(event: "vite:afterUpdate", callback: () => void): void;
	off(event: "vite:afterUpdate", callback: () => void): void;
}

declare global {
	interface Window {
		__TUIC_DEV_BACKEND__?: BuildIdentity;
	}
}

/** Only imported by the development entry point; never restarts the backend. */
export function startBackendSkewWarning(
	backend = window.__TUIC_DEV_BACKEND__,
	hot: HotUpdates | undefined = import.meta.hot,
): () => void {
	const container = document.createElement("div");
	document.body.append(container);
	const [warning, setWarning] = createSignal("");
	let dismissed = "";
	let currentKey = "";
	let generation = 0;
	let stopped = false;
	const dispose = render(
		() => (
			<Show when={warning()}>
				<div role="alert" class={styles.warning}>
					<span>{warning()}</span>
					<Button
						size="compact"
						variant="ghost"
						shape="icon"
						aria-label="Dismiss backend restart warning"
						onClick={() => {
							dismissed = currentKey;
							setWarning("");
						}}
					>
						×
					</Button>
				</div>
			</Show>
		),
		container,
	);

	async function check() {
		const request = ++generation;
		try {
			const sourceOrigin = new URL(import.meta.url).origin;
			const response = await fetch(`${sourceOrigin}/__tuic_backend_identity`, { cache: "no-store" });
			if (!response.ok) return;
			const tree = (await response.json()) as BuildIdentity;
			if (stopped || request !== generation || !tree.fingerprint || !tree.git_hash) return;
			if (backend?.fingerprint === tree.fingerprint) {
				dismissed = "";
				setWarning("");
				return;
			}
			currentKey = tree.fingerprint;
			if (dismissed === currentKey) return;
			setWarning(
				backend?.fingerprint
					? `Backend ${backend.git_hash}/${backend.fingerprint.slice(0, 12)} · tree ${tree.git_hash}/${tree.fingerprint.slice(0, 12)} · Restart tauri dev`
					: "Backend identity unavailable · Restart tauri dev once",
			);
		} catch {
			// A stopped/restarting Vite server is not evidence of backend skew.
		}
	}
	const onUpdate = () => {
		void check();
	};
	hot?.on("vite:afterUpdate", onUpdate);
	void check();
	const stop = () => {
		stopped = true;
		hot?.off("vite:afterUpdate", onUpdate);
		dispose();
		container.remove();
	};
	import.meta.hot?.dispose(stop);
	return stop;
}
