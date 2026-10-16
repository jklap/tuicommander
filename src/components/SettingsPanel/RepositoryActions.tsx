import { Show } from "solid-js";
import { t } from "../../i18n";
import s from "./Settings.module.css";
import { DangerZone, SectionCard } from "./SettingsPrimitives";
import r from "./tabs/RepositoryTabs.module.css";

/** Presentation only; the parent retains export and source-aware reset actions. */
export function RepositoryActions(props: {
	path: string;
	canExport: boolean;
	onExport: () => void;
	onReset: () => void;
}) {
	return (
		<div class={r.page} style={{ "margin-top": "16px" }}>
			<Show when={props.canExport}>
				<SectionCard title={t("settings.copyToProject.heading", "Share with Team")}>
					<p class={s.hint}>
						{t(
							"settings.copyToProject.hint",
							"Write this repo's worktree/branch settings to a .tuic.json in the project root. Commit it so teammates inherit the same defaults. Scripts are never exported.",
						)}
					</p>
					<div class={r.teamActions}>
						<button onClick={props.onExport}>
							{t("settings.copyToProject.button", "Copy settings to .tuic.json")}
						</button>
					</div>
				</SectionCard>
			</Show>
			<DangerZone
				title="Reset repository overrides"
				scope={props.path}
				consequence="Clear repository overrides, including script overrides, and return to inherited defaults. Repository identity and its Design Mode URL are retained."
			>
				<button class={s.footerReset} onClick={props.onReset}>
					{t("settings.resetToDefaults", "Reset to Defaults")}
				</button>
			</DangerZone>
		</div>
	);
}
