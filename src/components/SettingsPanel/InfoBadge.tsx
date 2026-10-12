import type { Component, JSX } from "solid-js";
import s from "./Settings.module.css";

const VIEWPORT_MARGIN = 8;
const TIP_GAP = 6;

/**
 * "?" badge with a hover tip. The tip is `position: fixed` and placed from the
 * badge rectangle, clamped to the viewport, so a scrolling ancestor
 * (`overflow-y: auto` makes overflow-x clip too) cannot cut it off.
 */
export const InfoBadge: Component<{ children: JSX.Element }> = (props) => {
	let tip!: HTMLSpanElement;

	const place = (event: MouseEvent) => {
		const badge = (event.currentTarget as HTMLElement).getBoundingClientRect();
		const box = tip.getBoundingClientRect();
		const maxLeft = window.innerWidth - box.width - VIEWPORT_MARGIN;
		const left = Math.max(VIEWPORT_MARGIN, Math.min(badge.left, maxLeft));
		const below = badge.bottom + TIP_GAP;
		const fitsBelow = below + box.height + VIEWPORT_MARGIN <= window.innerHeight;
		tip.style.left = `${left}px`;
		tip.style.top = `${fitsBelow ? below : Math.max(VIEWPORT_MARGIN, badge.top - TIP_GAP - box.height)}px`;
	};

	return (
		<span class={s.infoBadge} onMouseEnter={place}>
			?
			<span ref={tip} class={s.infoBadgeTip} role="tooltip">
				{props.children}
			</span>
		</span>
	);
};
