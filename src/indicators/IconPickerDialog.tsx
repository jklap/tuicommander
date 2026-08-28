import { type Component, createEffect, For, Show } from "solid-js";
import d from "../components/shared/dialog.module.css";
import { t } from "../i18n";
import { registerModal } from "../stores/modalStack";
import s from "./IconPickerDialog.module.css";
import { IndicatorIcon } from "./IndicatorIcon";
import { ICON_IDS, type IconId } from "./icons";

export interface IconPickerDialogProps {
	visible: boolean;
	title: string;
	currentIconId: IconId;
	onClose: () => void;
	onConfirm: (iconId: IconId) => void;
}

/**
 * Grid picker showing every curated icon shape rendered LIVE (the actual
 * glyph, not a name) — modeled on `ColorPickerDialog.tsx`'s overlay/popover
 * structure and `ColorSwatchPicker`'s "click a swatch to confirm and close"
 * interaction, generalized from color to icon shape.
 */
export const IconPickerDialog: Component<IconPickerDialogProps> = (props) => {
	createEffect(() => {
		if (!props.visible) return;
		registerModal(props.onClose);
	});

	return (
		<Show when={props.visible}>
			<div class={d.overlay} onClick={props.onClose}>
				<div class={d.popover} onClick={(e) => e.stopPropagation()}>
					<div class={d.header}>
						<h4>{props.title}</h4>
					</div>
					<div class={d.body}>
						<div class={s.grid}>
							<For each={ICON_IDS}>
								{(iconId) => (
									<button
										class={s.swatch}
										classList={{ [s.active]: iconId === props.currentIconId }}
										onClick={() => props.onConfirm(iconId)}
										title={iconId}
									>
										<IndicatorIcon id={iconId} size={18} />
									</button>
								)}
							</For>
						</div>
					</div>
					<div class={d.actions}>
						<button class={d.cancelBtn} onClick={props.onClose}>
							{t("iconPickerDialog.cancel", "Cancel")}
						</button>
					</div>
				</div>
			</div>
		</Show>
	);
};
