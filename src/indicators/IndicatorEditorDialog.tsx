import { type Component, createEffect, Show } from "solid-js";
import { ColorSwatchPicker } from "../components/shared/ColorSwatchPicker";
import { DEFAULT_COLOR_PRESETS } from "../components/shared/colorPresets";
import d from "../components/shared/dialog.module.css";
import { t } from "../i18n";
import { registerModal } from "../stores/modalStack";
import { settingsStore } from "../stores/settings";
import { AnimationOptionList } from "./AnimationOptionList";
import { IconSwatchGrid } from "./IconSwatchGrid";
import s from "./IndicatorEditorDialog.module.css";
import { IndicatorPreview } from "./IndicatorPreview";
import { getIndicator, resolveAnimationId, resolveIconId } from "./registry";

export interface IndicatorEditorDialogProps {
	/** The indicator being edited, or `null` when the dialog is closed. */
	indicatorId: string | null;
	onClose: () => void;
}

/**
 * The color/icon/animation editor for ONE indicator, combined into a single
 * dialog opened by clicking that indicator's own preview icon — replaces the
 * three separate per-row buttons (`UiLegend.tsx`'s old editSwatch/editIconBtn/
 * editAnimBtn) that each opened their own dialog. Every change here applies
 * live via the real `settingsStore` setters, same as the old per-field
 * dialogs did — there's no separate OK/Cancel, so the row's own
 * `IndicatorPreview` (shared with this dialog's own preview) updates as you
 * pick.
 */
export const IndicatorEditorDialog: Component<IndicatorEditorDialogProps> = (props) => {
	const entry = () => (props.indicatorId ? getIndicator(props.indicatorId) : undefined);

	createEffect(() => {
		if (!entry()) return;
		registerModal(props.onClose);
	});

	const overrideFor = () => settingsStore.state.indicatorOverrides.find((o) => o.id === props.indicatorId);

	/** Any field set at all — not just color — so "Reset to default" also
	 *  shows for an icon-only or animation-only override. */
	const hasOverride = (): boolean => {
		const o = overrideFor();
		return !!o && (o.color !== undefined || o.icon !== undefined || o.animation !== undefined);
	};

	return (
		<Show when={entry()}>
			{(e) => (
				<div class={d.overlay} onClick={props.onClose}>
					<div class={d.popover} onClick={(ev) => ev.stopPropagation()}>
						<div class={d.header}>
							<h4>{e().label}</h4>
						</div>
						<div class={d.body}>
							<div class={s.previewRow}>
								<IndicatorPreview entry={e()} />
								<span class={s.desc}>{e().description}</span>
							</div>

							<Show when={e().capabilities.includes("color")}>
								<div class={s.section}>
									<label class={s.sectionLabel}>{t("indicatorEditorDialog.color", "Color")}</label>
									<ColorSwatchPicker
										color={overrideFor()?.color ?? ""}
										presets={DEFAULT_COLOR_PRESETS}
										onChange={(color) => {
											if (color) settingsStore.setIndicatorColor(e().id, color);
											else settingsStore.clearIndicatorField(e().id, "color");
										}}
									/>
								</div>
							</Show>

							<Show when={e().capabilities.includes("icon")}>
								<div class={s.section}>
									<label class={s.sectionLabel}>{t("indicatorEditorDialog.icon", "Icon")}</label>
									<IconSwatchGrid
										currentIconId={resolveIconId(settingsStore.state.indicatorOverrides, e().id)}
										onSelect={(iconId) => settingsStore.setIndicatorIcon(e().id, iconId)}
									/>
								</div>
							</Show>

							<Show when={e().capabilities.includes("animation")}>
								<div class={s.section}>
									<label class={s.sectionLabel}>{t("indicatorEditorDialog.animation", "Animation")}</label>
									<AnimationOptionList
										currentAnimationId={resolveAnimationId(settingsStore.state.indicatorOverrides, e().id)}
										allowedAnimationIds={e().animations}
										onSelect={(animationId) => settingsStore.setIndicatorAnimation(e().id, animationId)}
									/>
								</div>
							</Show>
						</div>
						<div class={d.actions}>
							<Show when={hasOverride()}>
								<button class={d.cancelBtn} onClick={() => settingsStore.clearIndicatorOverride(e().id)}>
									{t("indicatorEditorDialog.resetToDefault", "Reset to default")}
								</button>
							</Show>
							<button class={d.primaryBtn} onClick={props.onClose}>
								{t("indicatorEditorDialog.close", "Close")}
							</button>
						</div>
					</div>
				</div>
			)}
		</Show>
	);
};
