import { type Component, For } from "solid-js";
import s from "./IconSwatchGrid.module.css";
import { IndicatorIcon } from "./IndicatorIcon";
import { ICON_IDS, type IconId } from "./icons";

export interface IconSwatchGridProps {
	currentIconId: IconId;
	onSelect: (iconId: IconId) => void;
}

/**
 * Grid of every curated icon shape rendered LIVE (the actual glyph, not a
 * name) — the icon section of `IndicatorEditorDialog`. Extracted from the
 * standalone `IconPickerDialog` (its overlay/popover shell moved into the
 * combined dialog); this component is just the grid body.
 */
export const IconSwatchGrid: Component<IconSwatchGridProps> = (props) => (
	<div class={s.grid}>
		<For each={ICON_IDS}>
			{(iconId) => (
				<button
					class={s.swatch}
					classList={{ [s.active]: iconId === props.currentIconId }}
					onClick={() => props.onSelect(iconId)}
					title={iconId}
				>
					<IndicatorIcon id={iconId} size={18} />
				</button>
			)}
		</For>
	</div>
);
