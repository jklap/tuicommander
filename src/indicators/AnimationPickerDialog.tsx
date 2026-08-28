import { type Component, createEffect, For, Show } from "solid-js";
import d from "../components/shared/dialog.module.css";
import { t } from "../i18n";
import { registerModal } from "../stores/modalStack";
import s from "./AnimationPickerDialog.module.css";
import { ANIMATION_LABELS, type AnimationId, INDICATOR_ANIMATIONS } from "./animations";

const ALL_ANIMATION_IDS = Object.keys(ANIMATION_LABELS) as AnimationId[];

export interface AnimationPickerDialogProps {
	visible: boolean;
	title: string;
	currentAnimationId: AnimationId;
	/** Narrows the offered choices — e.g. a badge doesn't offer "glow" (a
	 *  dot-shaped halo effect). Omit for the full set. */
	allowedAnimationIds?: readonly AnimationId[];
	onClose: () => void;
	onConfirm: (animationId: AnimationId) => void;
}

/**
 * List picker where every option animates its own live preview dot — so
 * "what does Breathe look like" is answered by watching it, not guessing
 * from the name. Same overlay/popover shell as ColorPickerDialog/IconPickerDialog.
 */
export const AnimationPickerDialog: Component<AnimationPickerDialogProps> = (props) => {
	createEffect(() => {
		if (!props.visible) return;
		registerModal(props.onClose);
	});

	const options = () => props.allowedAnimationIds ?? ALL_ANIMATION_IDS;

	return (
		<Show when={props.visible}>
			<div class={d.overlay} onClick={props.onClose}>
				<div class={d.popover} onClick={(e) => e.stopPropagation()}>
					<div class={d.header}>
						<h4>{props.title}</h4>
					</div>
					<div class={d.body}>
						<div class={s.list}>
							<For each={options()}>
								{(animationId) => (
									<button
										class={s.row}
										classList={{ [s.active]: animationId === props.currentAnimationId }}
										onClick={() => props.onConfirm(animationId)}
									>
										<span class={s.preview} style={{ animation: INDICATOR_ANIMATIONS[animationId] }} />
										<span class={s.label}>{ANIMATION_LABELS[animationId]}</span>
									</button>
								)}
							</For>
						</div>
					</div>
					<div class={d.actions}>
						<button class={d.cancelBtn} onClick={props.onClose}>
							{t("animationPickerDialog.cancel", "Cancel")}
						</button>
					</div>
				</div>
			</div>
		</Show>
	);
};
