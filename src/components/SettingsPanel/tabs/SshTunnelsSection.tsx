import type { Component } from "solid-js";
import { t } from "../../../i18n";
import type { TunnelProfile } from "../../../stores/tunnels";
import { TunnelProfileList } from "../../TunnelsPanel/TunnelProfileList";
import s from "../Settings.module.css";

/** "SSH Tunnels" section of the Remote Servers page: a heading, so search has
 * a scroll target, over the tunnel-profile list the Tunnels overlay shares. */
export const SshTunnelsSection: Component<{ onEdit: (profile: TunnelProfile) => void }> = (props) => (
	<div class={s.section}>
		<h3>{t("remoteServers.sshTunnels", "SSH Tunnels")}</h3>
		<TunnelProfileList onEdit={props.onEdit} />
	</div>
);
