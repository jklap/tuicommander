import type { Component } from "solid-js";
import { t } from "../../../i18n";
import s from "../Settings.module.css";
import { RemoteMachinesPanel } from "./services/RemoteMachinesPanel";

/** Top-level "Remote Machines" page: a heading (so search has a scroll
 * target) wrapping the existing connection-management panel. */
export const RemoteMachinesTab: Component = () => (
	<div class={s.section}>
		<h3>{t("settings.remoteMachines", "Remote Machines")}</h3>
		<RemoteMachinesPanel />
	</div>
);
