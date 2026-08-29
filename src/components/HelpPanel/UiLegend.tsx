import { type Component, createSignal, For, Show } from "solid-js";
import { t } from "../../i18n";
import { IndicatorEditorDialog } from "../../indicators/IndicatorEditorDialog";
import { IndicatorPreview } from "../../indicators/IndicatorPreview";
import { GROUP_HINTS, GROUP_LABELS, type IndicatorGroup, indicatorsByGroup } from "../../indicators/registry";
import { settingsStore } from "../../stores/settings";
import { SettingToggle } from "../SettingsPanel/SettingFields";
import s from "./UiLegend.module.css";

const GROUP_ORDER: readonly IndicatorGroup[] = [
	"terminalStatus",
	"tabType",
	"sidebarSymbol",
	"prBadge",
	"gitState",
	"diffStat",
];

/** Binds a group's visibility toggle to its settingsStore field. Groups not
 *  listed here (terminalStatus, sidebarSymbol) have no show/hide setting —
 *  those indicators aren't optional the way a whole badge/tint/section is. */
function groupToggleBinding(
	group: IndicatorGroup,
): { checked: boolean; onChange: (v: boolean) => void; label: string } | undefined {
	switch (group) {
		case "tabType":
			return {
				checked: settingsStore.state.tabTypeHighlighting,
				onChange: (v) => settingsStore.setTabTypeHighlighting(v),
				label: t("uiLegend.toggle.tabTypeHighlighting", "Show tab type highlighting"),
			};
		case "prBadge":
			return {
				checked: settingsStore.state.showPrBadges,
				onChange: (v) => settingsStore.setShowPrBadges(v),
				label: t("uiLegend.toggle.showPrBadges", "Show PR status badges"),
			};
		case "gitState":
			return {
				checked: settingsStore.state.showGitState,
				onChange: (v) => settingsStore.setShowGitState(v),
				label: t("uiLegend.toggle.showGitState", "Show git repo status indicators"),
			};
		case "diffStat":
			return {
				checked: settingsStore.state.showDiffStats,
				onChange: (v) => settingsStore.setShowDiffStats(v),
				label: t("uiLegend.toggle.showDiffStats", "Show diff stats"),
			};
		default:
			return undefined;
	}
}

/**
 * Visual reference for every color, icon, and animation used throughout
 * the app — rendered FROM `src/indicators/registry.ts`, the single source
 * of truth. Previously this component hand-maintained its own copy of
 * every color/label/description, which is exactly what let it drift from
 * the real components (see the customization plan's Context section for
 * the specific mismatches that caused — Busy shown as the wrong blue, PR
 * "Open" shown as the wrong color, sidebar symbols shown as text glyphs
 * the app hasn't rendered in a while, a whole "Panels" section describing
 * colors no panel applies, and more).
 *
 * `editable` turns each row's own preview icon into a button that opens
 * `IndicatorEditorDialog` — one combined dialog holding that row's color,
 * icon, and animation controls, whichever it has — plus a reset "×" that
 * clears the whole override. Used by Settings → Appearance.
 * `HelpPanel.tsx`'s reference view stays read-only: the preview there is
 * inert, not a button.
 */
export const UiLegend: Component<{ editable?: boolean }> = (props) => {
	const [editingId, setEditingId] = createSignal<string | null>(null);

	const overrideFor = (id: string) => settingsStore.state.indicatorOverrides.find((o) => o.id === id);

	/** Any field set at all — not just color — so the reset "×" also shows
	 *  for an icon-only or animation-only override. */
	const hasOverride = (id: string): boolean => {
		const o = overrideFor(id);
		return !!o && (o.color !== undefined || o.icon !== undefined || o.animation !== undefined);
	};

	return (
		<div class={s.legend}>
			<For each={GROUP_ORDER}>
				{(group) => (
					<div class={s.group}>
						<label class={s.groupLabel}>{GROUP_LABELS[group]}</label>
						<Show when={GROUP_HINTS[group]}>
							<p class={s.hint}>{GROUP_HINTS[group]}</p>
						</Show>
						<Show when={props.editable && groupToggleBinding(group)}>
							{(toggle) => (
								<SettingToggle checked={toggle().checked} onChange={toggle().onChange} label={toggle().label} />
							)}
						</Show>
						<div class={s.grid}>
							<For each={indicatorsByGroup(group)}>
								{(entry) => (
									<div class={s.row}>
										<Show when={props.editable} fallback={<IndicatorPreview entry={entry} />}>
											<button
												class={s.previewBtn}
												onClick={() => setEditingId(entry.id)}
												title={t("uiLegend.btn.customize", "Customize")}
											>
												<IndicatorPreview entry={entry} />
											</button>
										</Show>
										<span class={s.label}>{entry.label}</span>
										<span class={s.desc}>{entry.description}</span>
										<Show when={props.editable && hasOverride(entry.id)}>
											<button
												class={s.resetSwatch}
												onClick={() => settingsStore.clearIndicatorOverride(entry.id)}
												title={t("uiLegend.btn.resetOverride", "Reset to default")}
											>
												&times;
											</button>
										</Show>
									</div>
								)}
							</For>
						</div>
					</div>
				)}
			</For>

			<Show when={props.editable}>
				<button class={s.resetAllBtn} onClick={() => settingsStore.resetAllIndicators()}>
					{t("uiLegend.btn.resetAll", "Reset all indicators")}
				</button>
				<IndicatorEditorDialog indicatorId={editingId()} onClose={() => setEditingId(null)} />
			</Show>
		</div>
	);
};
