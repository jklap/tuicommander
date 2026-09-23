import { type Component, onMount, Show } from "solid-js";
import { registerModal } from "../../stores/modalStack";
import { tunnelPanelStore } from "../../stores/tunnelPanel";
import { tunnelsStore } from "../../stores/tunnels";
import { TunnelProfileList } from "./TunnelProfileList";
import s from "./TunnelsPanel.module.css";

export interface TunnelsPanelProps {
	/** Navigates to Settings' "Remote Servers" tab, which now owns tunnel
	 * config create/edit entirely (story: SSH Tunnels + Remote Servers
	 * consolidation, decision #1). Same mechanism as `McpPopup`'s
	 * "Manage in Settings" — `App.tsx`'s `openSettings(tab, section, target)`. */
	onOpenSettings?: (tab: string) => void;
}

export const TunnelsPanel: Component<TunnelsPanelProps> = (props) => {
	const isOpen = () => tunnelPanelStore.state.isOpen;

	onMount(() => {
		tunnelsStore.hydrate();
	});

	// Escape-to-close handled centrally (stores/modalStack), same convention as
	// every other dialog in this codebase (src/AGENTS.md's documented
	// anti-pattern: a bespoke document-level keydown listener here used to be
	// the one exception). registerModal is only "live" while the overlay is
	// actually open — this component itself is only ever mounted behind a
	// lazy <Show> in App.tsx, and its own visibility is `isOpen()`, so we
	// register/unregister as that flips rather than for the component's whole
	// (effectively permanent) lifetime.
	const closeOverlay = () => tunnelPanelStore.close();

	const editInSettings = () => {
		tunnelPanelStore.close();
		props.onOpenSettings?.("remote-servers");
	};

	return (
		<Show when={isOpen()}>
			<TunnelsPanelOverlay onClose={closeOverlay} onEditInSettings={editInSettings} />
		</Show>
	);
};

/** Split out so `registerModal` only registers while the overlay is actually
 * mounted (i.e. while `isOpen()` is true) — registering it unconditionally in
 * the outer, always-mounted `TunnelsPanel` would leave Escape permanently
 * routed here even while the overlay is closed. */
const TunnelsPanelOverlay: Component<{ onClose: () => void; onEditInSettings: () => void }> = (props) => {
	registerModal(props.onClose);

	return (
		<div class={s.overlay} onClick={(e) => e.target === e.currentTarget && props.onClose()}>
			<div class={s.dashboard}>
				{/* Header */}
				<div class={s.header}>
					<h3>SSH Tunnels</h3>
					<div class={s.headerActions}>
						<button class={s.newBtn} onClick={props.onEditInSettings}>
							Edit in Settings
						</button>
						<button class={s.closeBtn} onClick={props.onClose} title="Close">
							<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
								<path d="M3.72 3.72a.75.75 0 0 1 1.06 0L8 6.94l3.22-3.22a.75.75 0 1 1 1.06 1.06L9.06 8l3.22 3.22a.75.75 0 1 1-1.06 1.06L8 9.06l-3.22 3.22a.75.75 0 0 1-1.06-1.06L6.94 8 3.72 4.78a.75.75 0 0 1 0-1.06Z" />
							</svg>
						</button>
					</div>
				</div>

				{/* Profile list — Start/Stop/Log/Del only; Settings owns create/edit */}
				<TunnelProfileList />
			</div>
		</div>
	);
};

export default TunnelsPanel;
