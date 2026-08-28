import { type Component, For, type JSX } from "solid-js";
import { type IconDef, type IconId, type IconLayer, INDICATOR_ICON_DEFS } from "./icons";

/** One drawable layer, with fill/stroke set explicitly per-element rather
 *  than inherited from the parent <svg> — needed since a composite icon
 *  (e.g. `clock`) can mix filled and stroked layers. */
function renderLayer(layer: IconLayer): JSX.Element {
	switch (layer.kind) {
		case "circle":
			return <circle cx="8" cy="8" r={layer.r} fill="currentColor" />;
		case "ring":
			return <circle cx="8" cy="8" r={layer.r} fill="none" stroke="currentColor" stroke-width={layer.strokeWidth} />;
		case "arc":
			return (
				<circle
					cx="8"
					cy="8"
					r={layer.r}
					fill="none"
					stroke="currentColor"
					stroke-width={layer.strokeWidth}
					stroke-dasharray={layer.dashArray}
					stroke-linecap="round"
				/>
			);
		case "rect":
			return (
				<rect x={layer.x} y={layer.y} width={layer.width} height={layer.height} rx={layer.rx} fill="currentColor" />
			);
		case "stroke":
			return (
				<path
					d={layer.d}
					fill="none"
					stroke="currentColor"
					stroke-width={layer.strokeWidth}
					stroke-linecap="round"
					stroke-linejoin="round"
				/>
			);
		default:
			return <path d={layer.d} fill="currentColor" />;
	}
}

function layersFor(def: IconDef): readonly IconLayer[] {
	return def.kind === "composite" ? def.layers : [def];
}

/**
 * Renders one curated indicator icon shape. Color comes entirely from the
 * caller's CSS `color` (every layer uses `currentColor`) — this component
 * only decides the geometry. Mirrors `components/shared/SeverityIcon.tsx`'s
 * shared-icon idiom, generalized to a small per-layer shape system (circle,
 * ring, partial-ring arc, rect, filled path, stroked path) so a composite
 * icon like `clock` (ring + hands) or `pause` (two bars) can exist without
 * inventing a new component per shape.
 *
 * `size` is in pixels. Omit it (e.g. for the tab-bar dot, which used to be
 * a text glyph sized by `font-size`) to let the caller's CSS size the SVG
 * instead via `width/height: 1em` — keeps it in step with a state class
 * that changes font-size (see TabBar.module.css `.tabIcon`).
 */
export const IndicatorIcon: Component<{ id: IconId; size?: number; class?: string; style?: JSX.CSSProperties }> = (
	props,
) => {
	const layers = () => layersFor(INDICATOR_ICON_DEFS[props.id]);

	return (
		<svg
			class={props.class}
			style={props.style}
			viewBox="0 0 16 16"
			width={props.size}
			height={props.size}
			aria-hidden="true"
		>
			<For each={layers()}>{(layer) => renderLayer(layer)}</For>
		</svg>
	);
};
