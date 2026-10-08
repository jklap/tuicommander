import type { Component } from "solid-js";
import type { ConnectionStatus } from "../../stores/remoteConnections";

export interface ConnectionStatusBadgeProps {
	/** CSS color value (may be a `var(--x, #fallback)` comma-form custom property). */
	color: string;
	/** The text rendered next to the dot. */
	label: string;
	/** Optional hover title — defaults to `label`. */
	title?: string;
}

/**
 * One shared status-dot-plus-label presentation, used by both `TunnelStatusBadge`
 * (tunnel states: starting/connected/reconnecting/stopped/error) and the Remote
 * Servers list (remote-connection states: disconnected/connecting/deploying/connected/
 * unauthenticated/error).
 *
 * Extracted (story: SSH Tunnels + Remote Servers consolidation, "UI/copy/icon
 * inconsistencies") because those two were previously two independent
 * color/label implementations of the same connecting/connected/error/disconnected
 * concept — `TunnelStatusBadge.tsx`'s own dot markup and
 * `RemoteMachinesPanel.tsx`'s inline status-dot `<span>`. This component is pure
 * presentation: each call site still owns its own status vocabulary and maps it
 * to a `{color, label}` pair before rendering this.
 */
/** Status dot color for a remote connection's status — moved here from
 * `RemoteMachinesPanel.tsx`'s `remoteStatusColor`, vocabulary unchanged. */
export function remoteConnectionStatusColor(status: ConnectionStatus | string): string {
	switch (status) {
		case "connected":
			return "var(--success)";
		case "connecting":
		case "deploying":
			return "var(--activity)";
		// Reachable, but every route but /health answers 401 — the same amber as
		// "connecting" would read as progress, and green would be a lie.
		case "unauthenticated":
			return "var(--warning)";
		case "error":
			return "var(--error)";
		default:
			return "var(--fg-muted)";
	}
}

/** Human-readable label for a remote connection's status — moved here from
 * `RemoteMachinesPanel.tsx`'s `remoteStatusLabel`, vocabulary unchanged. */
export function remoteConnectionStatusLabel(status: ConnectionStatus | string, step?: string): string {
	switch (status) {
		case "connected":
			return "Connected";
		case "connecting":
			return "Connecting...";
		case "deploying":
			return `Deploying: ${step ?? "preparing"}`;
		case "unauthenticated":
			return "Not authenticated";
		case "error":
			return "Error";
		default:
			return "Disconnected";
	}
}

export const ConnectionStatusBadge: Component<ConnectionStatusBadgeProps> = (props) => (
	<span style={{ display: "inline-flex", "align-items": "center", gap: "5px", "font-size": "11px" }}>
		<span
			style={{
				width: "7px",
				height: "7px",
				"border-radius": "50%",
				background: props.color,
				"flex-shrink": "0",
			}}
			title={props.title ?? props.label}
		/>
		<span style={{ color: "var(--fg-secondary)" }}>{props.label}</span>
	</span>
);
