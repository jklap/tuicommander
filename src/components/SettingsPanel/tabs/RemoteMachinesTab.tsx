import type { Component } from "solid-js";
import { t } from "../../../i18n";
import s from "../Settings.module.css";
import { RemoteMachinesPanel, type RemoteMachinesPanelProps } from "./services/RemoteMachinesPanel";

/** "Remote Machines" section of the Remote Servers page (it was its own page
 * until the SSH Tunnels + Remote Machines merge): a heading, so search has a
 * scroll target, wrapping the connection list. */
export const RemoteMachinesTab: Component<RemoteMachinesPanelProps> = (props) => (
	<div class={s.section}>
		<h3>{t("settings.remoteMachines", "Remote Machines")}</h3>
		<RemoteMachinesPanel onEdit={props.onEdit} onAddFromHost={props.onAddFromHost} />
	</div>
);
