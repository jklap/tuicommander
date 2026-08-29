import { type Component, For } from "solid-js";
import s from "./AnimationOptionList.module.css";
import { ANIMATION_LABELS, type AnimationId, INDICATOR_ANIMATIONS } from "./animations";

const ALL_ANIMATION_IDS = Object.keys(ANIMATION_LABELS) as AnimationId[];

export interface AnimationOptionListProps {
	currentAnimationId: AnimationId;
	/** Narrows the offered choices — e.g. a badge doesn't offer "glow" (a
	 *  dot-shaped halo effect). Omit for the full set. */
	allowedAnimationIds?: readonly AnimationId[];
	onSelect: (animationId: AnimationId) => void;
}

/**
 * List of animation options where every option animates its own live
 * preview dot — so "what does Breathe look like" is answered by watching
 * it, not guessing from the name. The animation section of
 * `IndicatorEditorDialog`. Extracted from the standalone
 * `AnimationPickerDialog` (its overlay/popover shell moved into the
 * combined dialog); this component is just the list body.
 */
export const AnimationOptionList: Component<AnimationOptionListProps> = (props) => {
	const options = () => props.allowedAnimationIds ?? ALL_ANIMATION_IDS;

	return (
		<div class={s.list}>
			<For each={options()}>
				{(animationId) => (
					<button
						class={s.row}
						classList={{ [s.active]: animationId === props.currentAnimationId }}
						onClick={() => props.onSelect(animationId)}
					>
						<span class={s.preview} style={{ animation: INDICATOR_ANIMATIONS[animationId] }} />
						<span class={s.label}>{ANIMATION_LABELS[animationId]}</span>
					</button>
				)}
			</For>
		</div>
	);
};
