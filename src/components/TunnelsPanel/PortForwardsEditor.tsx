import { type Component, For } from "solid-js";
import type { ForwardSpec } from "../../stores/tunnels";
import s from "../SettingsPanel/Settings.module.css";
import d from "../shared/dialog.module.css";

export interface PortForwardsEditorProps {
	forwards: ForwardSpec[];
	onChange: (forwards: ForwardSpec[]) => void;
	/** Used as the default `remote_host` when a brand-new forward row is added. */
	defaultRemoteHost: string;
}

function emptyForward(): ForwardSpec {
	return { type: "Local", bind_port: 0 };
}

function parsePort(value: string, fallback: number): number {
	const parsed = Number.parseInt(value, 10);
	return Number.isFinite(parsed) ? parsed : fallback;
}

export function normalizeForwardForType(forward: ForwardSpec): ForwardSpec {
	if (forward.type === "Remote") {
		return {
			type: "Remote",
			bind_port: forward.bind_port,
			local_host: forward.local_host ?? forward.remote_host ?? "",
			local_port: forward.local_port ?? forward.remote_port ?? 0,
		};
	}

	return {
		type: "Local",
		bind_port: forward.bind_port,
		remote_host: forward.remote_host ?? forward.local_host ?? "",
		remote_port: forward.remote_port ?? forward.local_port ?? 0,
	};
}

export function convertForwardType(
	forward: ForwardSpec,
	type: ForwardSpec["type"],
	defaultRemoteHost = "",
): ForwardSpec {
	if (type === "Remote") {
		return {
			type,
			bind_port: forward.bind_port,
			local_host: forward.local_host ?? "127.0.0.1",
			local_port: forward.local_port ?? forward.remote_port ?? 0,
		};
	}

	return {
		type,
		bind_port: forward.bind_port,
		remote_host: forward.remote_host ?? defaultRemoteHost,
		remote_port: forward.remote_port ?? forward.local_port ?? 0,
	};
}

/**
 * Port-forward list editor — add/remove/edit `ForwardSpec` rows for an SSH
 * tunnel profile. Extracted from `TunnelEditorModal.tsx` (story: SSH Tunnels
 * + Remote Servers consolidation) so the merged Settings connection editor's
 * "SSH Tunnel" kind section reuses the exact same UI instead of a second copy.
 */
export const PortForwardsEditor: Component<PortForwardsEditorProps> = (props) => {
	const addForward = () =>
		props.onChange([...props.forwards, { ...emptyForward(), remote_host: props.defaultRemoteHost || undefined }]);
	const removeForward = (idx: number) => props.onChange(props.forwards.filter((_, i) => i !== idx));
	const updateForward = (idx: number, patch: Partial<ForwardSpec>) => {
		props.onChange(props.forwards.map((fw, i) => (i === idx ? { ...fw, ...patch } : fw)));
	};
	const replaceForward = (idx: number, next: ForwardSpec) => {
		props.onChange(props.forwards.map((fw, i) => (i === idx ? next : fw)));
	};

	return (
		<div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
			<div style={{ display: "flex", "align-items": "center", "justify-content": "space-between" }}>
				<label class={s.label}>Port Forwards</label>
				<button
					type="button"
					class={d.cancelBtn}
					style={{
						flex: "none",
						padding: "2px 10px",
						"font-size": "var(--font-sm)",
						border: "none",
						"border-radius": "var(--radius-md)",
						cursor: "pointer",
					}}
					onClick={addForward}
				>
					+ Add
				</button>
			</div>
			<For each={props.forwards}>
				{(fw, idx) => (
					<div class={s.group} style={{ display: "flex", gap: "6px", "align-items": "center" }}>
						<select
							style={{ width: "80px" }}
							value={fw.type}
							onChange={(e) =>
								replaceForward(
									idx(),
									convertForwardType(fw, e.currentTarget.value as ForwardSpec["type"], props.defaultRemoteHost),
								)
							}
						>
							<option value="Local">Local</option>
							<option value="Remote">Remote</option>
						</select>
						<input
							inputMode="numeric"
							pattern="[0-9]*"
							placeholder="bind"
							style={{ width: "70px" }}
							value={fw.bind_port || ""}
							onInput={(e) =>
								updateForward(idx(), {
									bind_port: parsePort(e.currentTarget.value, 0),
								})
							}
						/>
						<span style={{ color: "var(--fg-muted)" }}>:</span>
						<input
							placeholder={fw.type === "Remote" ? "local host" : "remote host"}
							style={{ flex: "1" }}
							value={fw.type === "Remote" ? (fw.local_host ?? fw.remote_host ?? "") : (fw.remote_host ?? "")}
							onInput={(e) =>
								updateForward(
									idx(),
									fw.type === "Remote" ? { local_host: e.currentTarget.value } : { remote_host: e.currentTarget.value },
								)
							}
						/>
						<span style={{ color: "var(--fg-muted)" }}>:</span>
						<input
							inputMode="numeric"
							pattern="[0-9]*"
							placeholder="port"
							style={{ width: "70px" }}
							value={fw.type === "Remote" ? (fw.local_port ?? fw.remote_port ?? "") : (fw.remote_port ?? "")}
							onInput={(e) =>
								updateForward(
									idx(),
									fw.type === "Remote"
										? { local_port: parsePort(e.currentTarget.value, 0) }
										: { remote_port: parsePort(e.currentTarget.value, 0) },
								)
							}
						/>
						<button
							type="button"
							class={d.cancelBtn}
							style={{
								flex: "none",
								padding: "2px 8px",
								"font-size": "var(--font-sm)",
								border: "none",
								"border-radius": "var(--radius-md)",
								cursor: "pointer",
							}}
							onClick={() => removeForward(idx())}
						>
							x
						</button>
					</div>
				)}
			</For>
		</div>
	);
};
