import type { Component } from "solid-js";
import { t } from "../../../i18n";
import { SectionCard } from "../SettingsPrimitives";
import { RemoteMachinesPanel, type RemoteMachinesPanelProps } from "./services/RemoteMachinesPanel";

/** "Remote Machines" section of the Remote Servers page (it was its own page
 * until the SSH Tunnels + Remote Machines merge): a section card, so search has
 * a scroll target, wrapping the connection list. */
export const RemoteMachinesTab: Component<RemoteMachinesPanelProps> = (props) => (
	<SectionCard
		title={t("settings.remoteMachines", "Remote Machines")}
		description="Connect to named SSH and direct daemons; manage their deployment and lifetime."
	>
		<RemoteMachinesPanel onEdit={props.onEdit} onAddFromHost={props.onAddFromHost} />
	</SectionCard>
);
