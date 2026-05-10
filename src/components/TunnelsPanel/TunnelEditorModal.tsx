import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import type { ForwardSpec, ProfileOptions, TunnelProfile } from "../../stores/tunnels";
import { tunnelsStore } from "../../stores/tunnels";

interface TunnelEditorModalProps {
	profile?: TunnelProfile;
	onClose: () => void;
}

function defaultOptions(): ProfileOptions {
	return {
		server_alive_interval: 15,
		server_alive_count_max: 3,
		strict_host_key_checking: "Yes",
	};
}

function emptyForward(): ForwardSpec {
	return { type: "Local", bind_port: 0 };
}

export const TunnelEditorModal: Component<TunnelEditorModalProps> = (props) => {
	const isEdit = () => !!props.profile;

	const [name, setName] = createSignal(props.profile?.name ?? "");
	const [host, setHost] = createSignal(props.profile?.host ?? "");
	const [port, setPort] = createSignal(props.profile?.port ?? 22);
	const [user, setUser] = createSignal(props.profile?.user ?? "");
	const [identityFile, setIdentityFile] = createSignal(props.profile?.identity_file ?? "");
	const [forwards, setForwards] = createSignal<ForwardSpec[]>(props.profile?.forwards ?? []);
	const [options, setOptions] = createSignal<ProfileOptions>(props.profile?.options ?? defaultOptions());
	const [saving, setSaving] = createSignal(false);
	const [error, setError] = createSignal("");
	const [sshHosts, setSshHosts] = createSignal<string[]>([]);

	onMount(async () => {
		try {
			const hosts = await invoke<string[]>("list_ssh_config_hosts");
			if (hosts?.length) setSshHosts(hosts);
		} catch {
			// ssh config parsing is best-effort
		}
	});

	const addForward = () => setForwards((f) => [...f, emptyForward()]);
	const removeForward = (idx: number) => setForwards((f) => f.filter((_, i) => i !== idx));
	const updateForward = (idx: number, patch: Partial<ForwardSpec>) => {
		setForwards((f) => f.map((fw, i) => (i === idx ? { ...fw, ...patch } : fw)));
	};

	const handleSave = async () => {
		const trimmedName = name().trim();
		const trimmedHost = host().trim();
		const trimmedUser = user().trim();

		if (!trimmedName || !trimmedHost || !trimmedUser) {
			setError("Name, host, and user are required.");
			return;
		}

		setSaving(true);
		setError("");

		try {
			const data = {
				name: trimmedName,
				host: trimmedHost,
				port: port(),
				user: trimmedUser,
				identity_file: identityFile().trim() || null,
				forwards: forwards(),
				options: options(),
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

	const overlayStyle = {
		position: "fixed",
		inset: "0",
		background: "rgba(0,0,0,0.6)",
		display: "flex",
		"align-items": "center",
		"justify-content": "center",
		"z-index": "1100",
	} as const;

	const modalStyle = {
		width: "520px",
		"max-width": "90vw",
		"max-height": "80vh",
		background: "var(--bg-secondary)",
		"border-radius": "var(--radius-xl)",
		border: "1px solid var(--border)",
		"box-shadow": "var(--shadow-popup)",
		display: "flex",
		"flex-direction": "column",
		overflow: "hidden",
	} as const;

	const headerStyle = {
		padding: "12px 16px",
		"border-bottom": "1px solid var(--border)",
		"font-size": "var(--font-lg)",
		"font-weight": "600",
		margin: "0",
	} as const;

	const bodyStyle = {
		padding: "16px",
		"overflow-y": "auto",
		flex: "1",
		display: "flex",
		"flex-direction": "column",
		gap: "12px",
	} as const;

	const labelStyle = {
		display: "flex",
		"flex-direction": "column",
		gap: "4px",
		"font-size": "var(--font-sm)",
		color: "var(--fg-secondary)",
	} as const;

	const inputStyle = {
		background: "var(--bg-primary)",
		border: "1px solid var(--border)",
		"border-radius": "var(--radius-sm)",
		padding: "6px 8px",
		color: "var(--fg-primary)",
		"font-size": "var(--font-md)",
	} as const;

	const rowStyle = {
		display: "flex",
		gap: "8px",
		"align-items": "flex-end",
	} as const;

	const footerStyle = {
		display: "flex",
		"align-items": "center",
		"justify-content": "flex-end",
		gap: "8px",
		padding: "12px 16px",
		"border-top": "1px solid var(--border)",
	} as const;

	const btnStyle = (primary?: boolean) =>
		({
			padding: "6px 16px",
			"border-radius": "var(--radius-sm)",
			border: primary ? "none" : "1px solid var(--border)",
			background: primary ? "var(--accent)" : "var(--bg-tertiary)",
			color: primary ? "#fff" : "var(--fg-primary)",
			cursor: "pointer",
			"font-size": "var(--font-sm)",
		}) as const;

	const smallBtnStyle = {
		background: "none",
		border: "none",
		color: "var(--fg-muted)",
		cursor: "pointer",
		"font-size": "var(--font-sm)",
		padding: "2px 6px",
	} as const;

	return (
		<div style={overlayStyle} onClick={(e) => e.target === e.currentTarget && props.onClose()}>
			<div style={modalStyle}>
				<h3 style={headerStyle}>{isEdit() ? "Edit Tunnel" : "New Tunnel"}</h3>

				<div style={bodyStyle}>
					{/* Name */}
					<label style={labelStyle}>
						Name
						<input style={inputStyle} value={name()} onInput={(e) => setName(e.currentTarget.value)} />
					</label>

					{/* Host + Port */}
					<div style={rowStyle}>
						<label style={{ ...labelStyle, flex: "1" }}>
							Host
							<input
								style={inputStyle}
								value={host()}
								onInput={(e) => setHost(e.currentTarget.value)}
								list="ssh-hosts-list"
							/>
							<datalist id="ssh-hosts-list">
								<For each={sshHosts()}>{(h) => <option value={h} />}</For>
							</datalist>
						</label>
						<label style={{ ...labelStyle, width: "80px" }}>
							Port
							<input
								style={inputStyle}
								type="number"
								value={port()}
								onInput={(e) => setPort(Number.parseInt(e.currentTarget.value, 10) || 22)}
							/>
						</label>
					</div>

					{/* User */}
					<label style={labelStyle}>
						User
						<input style={inputStyle} value={user()} onInput={(e) => setUser(e.currentTarget.value)} />
					</label>

					{/* Identity File */}
					<label style={labelStyle}>
						Identity File (optional)
						<input
							style={inputStyle}
							value={identityFile()}
							onInput={(e) => setIdentityFile(e.currentTarget.value)}
							placeholder="~/.ssh/id_rsa"
						/>
					</label>

					{/* Forwards */}
					<div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
						<div style={{ display: "flex", "align-items": "center", "justify-content": "space-between" }}>
							<span style={{ "font-size": "var(--font-sm)", color: "var(--fg-secondary)" }}>Port Forwards</span>
							<button type="button" style={smallBtnStyle} onClick={addForward}>
								+ Add
							</button>
						</div>
						<For each={forwards()}>
							{(fw, idx) => (
								<div style={{ ...rowStyle, "align-items": "center" }}>
									<select
										style={{ ...inputStyle, width: "80px" }}
										value={fw.type}
										onChange={(e) => updateForward(idx(), { type: e.currentTarget.value as "Local" | "Remote" })}
									>
										<option value="Local">Local</option>
										<option value="Remote">Remote</option>
									</select>
									<input
										style={{ ...inputStyle, width: "70px" }}
										type="number"
										placeholder="bind"
										value={fw.bind_port || ""}
										onInput={(e) =>
											updateForward(idx(), {
												bind_port: Number.parseInt(e.currentTarget.value, 10) || 0,
											})
										}
									/>
									<span style={{ color: "var(--fg-muted)", "font-size": "var(--font-sm)" }}>:</span>
									<input
										style={{ ...inputStyle, flex: "1" }}
										placeholder="remote host"
										value={fw.remote_host ?? ""}
										onInput={(e) => updateForward(idx(), { remote_host: e.currentTarget.value })}
									/>
									<span style={{ color: "var(--fg-muted)", "font-size": "var(--font-sm)" }}>:</span>
									<input
										style={{ ...inputStyle, width: "70px" }}
										type="number"
										placeholder="port"
										value={fw.remote_port ?? ""}
										onInput={(e) =>
											updateForward(idx(), {
												remote_port: Number.parseInt(e.currentTarget.value, 10) || 0,
											})
										}
									/>
									<button type="button" style={smallBtnStyle} onClick={() => removeForward(idx())}>
										x
									</button>
								</div>
							)}
						</For>
					</div>

					{/* Options */}
					<div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
						<span style={{ "font-size": "var(--font-sm)", color: "var(--fg-secondary)" }}>Options</span>
						<div style={rowStyle}>
							<label style={{ ...labelStyle, flex: "1" }}>
								ServerAliveInterval
								<input
									style={inputStyle}
									type="number"
									value={options().server_alive_interval}
									onInput={(e) =>
										setOptions((o) => ({
											...o,
											server_alive_interval: Number.parseInt(e.currentTarget.value, 10) || 15,
										}))
									}
								/>
							</label>
							<label style={{ ...labelStyle, width: "140px" }}>
								StrictHostKeyChecking
								<select
									style={inputStyle}
									value={options().strict_host_key_checking}
									onChange={(e) =>
										setOptions((o) => ({
											...o,
											strict_host_key_checking: e.currentTarget.value as "Yes" | "AcceptNew",
										}))
									}
								>
									<option value="AcceptNew">AcceptNew</option>
									<option value="Yes">Yes</option>
								</select>
							</label>
						</div>
					</div>

					{/* Error */}
					<Show when={error()}>
						<div style={{ color: "var(--accent-red, #ef4444)", "font-size": "var(--font-sm)" }}>{error()}</div>
					</Show>
				</div>

				{/* Footer */}
				<div style={footerStyle}>
					<button type="button" style={btnStyle()} onClick={props.onClose}>
						Cancel
					</button>
					<button type="button" style={btnStyle(true)} onClick={handleSave} disabled={saving()}>
						{saving() ? "Saving..." : "Save"}
					</button>
				</div>
			</div>
		</div>
	);
};
