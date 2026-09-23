import { type Component, createSignal, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { registerModal } from "../../stores/modalStack";
import type { ForwardSpec, SshConnectionParams, TunnelProfile } from "../../stores/tunnels";
import { tunnelsStore } from "../../stores/tunnels";
import s from "../SettingsPanel/Settings.module.css";
import d from "../shared/dialog.module.css";
import { SshConnectionFields } from "../shared/SshConnectionFields";
import { normalizeForwardForType, PortForwardsEditor } from "./PortForwardsEditor";

interface TunnelEditorModalProps {
	profile?: TunnelProfile;
	onClose: () => void;
}

function defaultSsh(): SshConnectionParams {
	return {
		host: "",
		port: 22,
		user: "",
		identity_file: null,
		server_alive_interval: 15,
		server_alive_count_max: 3,
		strict_host_key_checking: "Yes",
	};
}

// Re-exported for backward compatibility — these pure forward-shaping helpers
// now live in `PortForwardsEditor.tsx` (extracted so the merged Settings
// connection editor's "SSH Tunnel" kind section can share the identical
// Port Forwards UI, not just the shaping logic).
export { convertForwardType, normalizeForwardForType } from "./PortForwardsEditor";

export const TunnelEditorModal: Component<TunnelEditorModalProps> = (props) => {
	const isEdit = () => !!props.profile;
	// Escape-to-close handled centrally (stores/modalStack): closes this modal and
	// keeps Escape from reaching the terminal underneath.
	registerModal(props.onClose);

	const [name, setName] = createSignal(props.profile?.name ?? "");
	const [ssh, setSsh] = createSignal<SshConnectionParams>(props.profile?.ssh ?? defaultSsh());
	const [forwards, setForwards] = createSignal<ForwardSpec[]>(props.profile?.forwards ?? []);
	const [autoConnect, setAutoConnect] = createSignal(props.profile?.auto_connect ?? false);
	const [saving, setSaving] = createSignal(false);
	const [error, setError] = createSignal("");

	const patchSsh = (patch: Partial<SshConnectionParams>) => setSsh((cur) => ({ ...cur, ...patch }));

	const handleSave = async () => {
		const trimmedName = name().trim();
		const trimmedHost = ssh().host.trim();
		const trimmedUser = ssh().user.trim();

		if (!trimmedName || !trimmedHost || !trimmedUser) {
			setError("Name, host, and user are required.");
			return;
		}

		setSaving(true);
		setError("");

		try {
			const data = {
				name: trimmedName,
				ssh: {
					...ssh(),
					host: trimmedHost,
					user: trimmedUser,
					identity_file: ssh().identity_file?.trim() || null,
				},
				forwards: forwards().map(normalizeForwardForType),
				auto_connect: autoConnect(),
			};

			if (isEdit() && props.profile) {
				await tunnelsStore.updateProfile({ id: props.profile.id, ...data });
			} else {
				await tunnelsStore.createProfile(data);
			}
			props.onClose();
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			setError(msg);
			appLogger.error("store", "TunnelEditor save failed", err);
		} finally {
			setSaving(false);
		}
	};

	return (
		<div class={d.overlay} onClick={(e) => e.target === e.currentTarget && props.onClose()}>
			<div class={d.popover} style={{ width: "520px" }}>
				<div class={d.header}>
					<h4>{isEdit() ? "Edit Tunnel" : "New Tunnel"}</h4>
				</div>

				<div
					class={d.body}
					style={{
						display: "flex",
						"flex-direction": "column",
						gap: "12px",
						"max-height": "60vh",
						"overflow-y": "auto",
					}}
				>
					<div class={s.group}>
						<label class={s.label}>Name</label>
						<input value={name()} onInput={(e) => setName(e.currentTarget.value)} />
					</div>

					<SshConnectionFields value={ssh()} onChange={patchSsh} />

					<PortForwardsEditor forwards={forwards()} onChange={setForwards} defaultRemoteHost={ssh().host.trim()} />

					<label class={s.toggle}>
						<input type="checkbox" checked={autoConnect()} onChange={(e) => setAutoConnect(e.currentTarget.checked)} />
						<span>Connect automatically on startup</span>
					</label>

					<Show when={error()}>
						<div
							class={d.error}
							ref={(el) => requestAnimationFrame(() => el.scrollIntoView({ behavior: "smooth", block: "nearest" }))}
						>
							{error()}
						</div>
					</Show>
				</div>

				<div class={d.actions}>
					<button class={d.cancelBtn} onClick={props.onClose}>
						Cancel
					</button>
					<button class={d.primaryBtn} onClick={handleSave} disabled={saving()}>
						{saving() ? "Saving..." : "Save"}
					</button>
				</div>
			</div>
		</div>
	);
};
