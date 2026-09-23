import type { Component } from "solid-js";
import { t } from "../../../i18n";
import type { TunnelProfile } from "../../../stores/tunnels";
import { TunnelProfileList } from "../../TunnelsPanel/TunnelProfileList";
import { SectionCard } from "../SettingsPrimitives";

/** "SSH Tunnels" section of the Remote Servers page: a section card, so search has
 * a scroll target, over the tunnel-profile list the Tunnels overlay shares. */
export const SshTunnelsSection: Component<{ onEdit: (profile: TunnelProfile) => void }> = (props) => (
	<SectionCard title={t("remoteServers.sshTunnels", "SSH Tunnels")}>
		<TunnelProfileList onEdit={props.onEdit} />
	</SectionCard>
);
