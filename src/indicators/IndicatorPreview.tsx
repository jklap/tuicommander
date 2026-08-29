import { type Component, Show } from "solid-js";
import { settingsStore } from "../stores/settings";
import { IndicatorIcon } from "./IndicatorIcon";
import s from "./IndicatorPreview.module.css";
import { type IndicatorDef, resolveIconId } from "./registry";

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
 * One indicator's preview swatch — shared by the legend row and the
 * combined edit dialog so both always show the exact same thing. Shape
 * follows `entry.preview`, but a capability-icon entry always renders its
 * REAL, override-aware shape (`IndicatorIcon`) instead of a generic dot —
 * this is what fixed the old legend showing "✱"/"⎇" text glyphs while the
 * sidebar actually renders SVG paths, and what makes a picked icon show up
 * here immediately instead of only in the picker.
 *
 * Gated on `entry.capabilities.includes("icon")`, not on `defaultIconId` —
 * `resolveIconId` falls back to `"dot"` for a non-icon-capable entry, which
 * would wrongly turn every bar/badge preview into a dot icon.
 */
export const IndicatorPreview: Component<{ entry: IndicatorDef }> = (props) => {
	const color = () => resolvedColor(props.entry);
	const animation = () => resolvedAnimation(props.entry);
	const iconId = () => resolveIconId(settingsStore.state.indicatorOverrides, props.entry.id);

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
				when={props.entry.capabilities.includes("icon")}
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
				<IndicatorIcon
					id={iconId()}
					size={14}
					class={s.previewIcon}
					style={{ color: color(), animation: animation() }}
				/>
			</Show>
		</Show>
	);
};
