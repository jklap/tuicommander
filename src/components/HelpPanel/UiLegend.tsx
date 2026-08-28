import { type Component, createSignal, For, Show } from "solid-js";
import { t } from "../../i18n";
import { AnimationPickerDialog } from "../../indicators/AnimationPickerDialog";
import { IconPickerDialog } from "../../indicators/IconPickerDialog";
import { IndicatorIcon } from "../../indicators/IndicatorIcon";
import {
	GROUP_HINTS,
	GROUP_LABELS,
	getIndicator,
	type IndicatorDef,
	type IndicatorGroup,
	indicatorsByGroup,
	resolveAnimationId,
	resolveIconId,
} from "../../indicators/registry";
import { settingsStore } from "../../stores/settings";
import { SettingToggle } from "../SettingsPanel/SettingFields";
import { ColorPickerDialog } from "../shared/ColorPickerDialog";
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

/** diffStat previews are a literal glyph (+N / -N), not a shape — the only
 *  group where the "preview" is the thing users actually see in the UI
 *  rather than a stand-in for it. Presentation-only; doesn't need
 *  registry-level modeling. */
const DIFF_STAT_GLYPH: Record<string, string> = {
	"diffStat.additions": "+N",
	"diffStat.deletions": "-N",
};

/** tabType's colorVar is a raw "r, g, b" triple (consumed inside rgba() so
 *  tint gradients can vary alpha) — every other group's colorVar is a
 *  ready-to-use color. */
function resolvedColor(entry: IndicatorDef): string | undefined {
	if (!entry.colorVar) return undefined;
	return entry.group === "tabType" ? `rgb(var(${entry.colorVar}))` : `var(${entry.colorVar})`;
}

function resolvedAnimation(entry: IndicatorDef): string | undefined {
	return entry.animVar ? `var(${entry.animVar})` : undefined;
}

/**
 * One legend row's preview swatch. Shape follows `entry.preview`, but an
 * entry with an icon always renders its REAL shape (IndicatorIcon) instead
 * of a generic dot — this is what fixed the old legend showing "✱"/"⎇" text
 * glyphs while the sidebar actually renders SVG paths.
 */
const IndicatorPreview: Component<{ entry: IndicatorDef }> = (props) => {
	const color = () => resolvedColor(props.entry);
	const animation = () => resolvedAnimation(props.entry);

	return (
		<Show
			when={props.entry.group !== "diffStat"}
			fallback={
				<span class={s.symbol} style={{ color: color() }}>
					{DIFF_STAT_GLYPH[props.entry.id]}
				</span>
			}
		>
			<Show
				when={props.entry.defaultIconId}
				fallback={
					props.entry.preview === "bar" ? (
						<span class={s.colorBar} style={{ background: color() }} />
					) : props.entry.preview === "badge" ? (
						<span class={s.badge} style={{ background: color(), animation: animation() }} />
					) : (
						<span class={s.dot} style={{ background: color(), animation: animation() }} />
					)
				}
			>
				{(iconId) => (
					<IndicatorIcon
						id={iconId()}
						size={14}
						class={s.previewIcon}
						style={{ color: color(), animation: animation() }}
					/>
				)}
			</Show>
		</Show>
	);
};

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
 * `editable` turns each color/icon/animation-capable row into an editor — a
 * swatch button per capability, opening the matching picker dialog, plus a
 * reset "×" that clears the whole override — used by Settings → Appearance.
 * `HelpPanel.tsx`'s reference view stays read-only.
 */
export const UiLegend: Component<{ editable?: boolean }> = (props) => {
	const [editingColorId, setEditingColorId] = createSignal<string | null>(null);
	const [editingIconId, setEditingIconId] = createSignal<string | null>(null);
	const [editingAnimationId, setEditingAnimationId] = createSignal<string | null>(null);

	const overrideFor = (id: string) => settingsStore.state.indicatorOverrides.find((o) => o.id === id);

	const overrideColorFor = (id: string): string => overrideFor(id)?.color ?? "";

	const currentIconIdFor = (id: string) => resolveIconId(settingsStore.state.indicatorOverrides, id);

	const currentAnimationIdFor = (id: string) => resolveAnimationId(settingsStore.state.indicatorOverrides, id);

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
										<IndicatorPreview entry={entry} />
										<span class={s.label}>{entry.label}</span>
										<span class={s.desc}>{entry.description}</span>
										<Show when={props.editable}>
											<Show when={entry.capabilities.includes("color")}>
												<button
													class={s.editSwatch}
													style={{ background: resolvedColor(entry) }}
													onClick={() => setEditingColorId(entry.id)}
													title={t("uiLegend.btn.changeColor", "Change color")}
												/>
											</Show>
											<Show when={entry.capabilities.includes("icon")}>
												<button
													class={s.editIconBtn}
													onClick={() => setEditingIconId(entry.id)}
													title={t("uiLegend.btn.changeIcon", "Change icon")}
												>
													<IndicatorIcon id={currentIconIdFor(entry.id)} size={14} />
												</button>
											</Show>
											<Show when={entry.capabilities.includes("animation")}>
												<button
													class={s.editAnimBtn}
													onClick={() => setEditingAnimationId(entry.id)}
													title={t("uiLegend.btn.changeAnimation", "Change animation")}
												>
													{currentAnimationIdFor(entry.id)}
												</button>
											</Show>
											<Show when={hasOverride(entry.id)}>
												<button
													class={s.resetSwatch}
													onClick={() => settingsStore.clearIndicatorOverride(entry.id)}
													title={t("uiLegend.btn.resetOverride", "Reset to default")}
												>
													&times;
												</button>
											</Show>
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
				<ColorPickerDialog
					visible={editingColorId() !== null}
					title={t("uiLegend.dialog.indicatorColor", "Indicator Color")}
					currentColor={editingColorId() ? overrideColorFor(editingColorId()!) : ""}
					onClose={() => setEditingColorId(null)}
					onConfirm={(color) => {
						const id = editingColorId();
						if (!id) return;
						if (color) settingsStore.setIndicatorColor(id, color);
						else settingsStore.clearIndicatorOverride(id);
						setEditingColorId(null);
					}}
				/>
				<IconPickerDialog
					visible={editingIconId() !== null}
					title={t("uiLegend.dialog.indicatorIcon", "Indicator Icon")}
					currentIconId={editingIconId() ? currentIconIdFor(editingIconId()!) : "dot"}
					onClose={() => setEditingIconId(null)}
					onConfirm={(iconId) => {
						const id = editingIconId();
						if (!id) return;
						settingsStore.setIndicatorIcon(id, iconId);
						setEditingIconId(null);
					}}
				/>
				<AnimationPickerDialog
					visible={editingAnimationId() !== null}
					title={t("uiLegend.dialog.indicatorAnimation", "Indicator Animation")}
					currentAnimationId={editingAnimationId() ? currentAnimationIdFor(editingAnimationId()!) : "none"}
					allowedAnimationIds={editingAnimationId() ? getIndicator(editingAnimationId()!)?.animations : undefined}
					onClose={() => setEditingAnimationId(null)}
					onConfirm={(animationId) => {
						const id = editingAnimationId();
						if (!id) return;
						settingsStore.setIndicatorAnimation(id, animationId);
						setEditingAnimationId(null);
					}}
				/>
			</Show>
		</div>
	);
};
