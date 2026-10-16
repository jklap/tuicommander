import { type Component, createSignal, createUniqueId, type JSX, onCleanup, onMount } from "solid-js";
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
	let badgeElement!: HTMLSpanElement;
	const id = createUniqueId();
	const [hovered, setHovered] = createSignal(false);
	const [focused, setFocused] = createSignal(false);
	const [positioned, setPositioned] = createSignal(false);

	const place = () => {
		const badge = badgeElement.getBoundingClientRect();
		const box = tip.getBoundingClientRect();
		const maxLeft = window.innerWidth - box.width - VIEWPORT_MARGIN;
		const left = Math.max(VIEWPORT_MARGIN, Math.min(badge.left, maxLeft));
		const below = badge.bottom + TIP_GAP;
		const fitsBelow = below + box.height + VIEWPORT_MARGIN <= window.innerHeight;
		tip.style.left = `${left}px`;
		tip.style.top = `${fitsBelow ? below : Math.max(VIEWPORT_MARGIN, badge.top - TIP_GAP - box.height)}px`;
		setPositioned(true);
	};

	onMount(() => {
		const reposition = () => {
			if (hovered() || focused()) place();
		};
		const observer = new ResizeObserver(reposition);
		observer.observe(tip);
		window.addEventListener("resize", reposition);
		window.addEventListener("scroll", reposition, true);
		onCleanup(() => {
			observer.disconnect();
			window.removeEventListener("resize", reposition);
			window.removeEventListener("scroll", reposition, true);
		});
	});

	return (
		<span
			ref={badgeElement}
			class={s.infoBadge}
			tabIndex={0}
			aria-describedby={id}
			onMouseEnter={() => {
				place();
				setHovered(true);
			}}
			onMouseLeave={() => setHovered(false)}
			onFocus={() => {
				place();
				setFocused(true);
			}}
			onBlur={() => setFocused(false)}
		>
			?
			<span
				ref={tip}
				id={id}
				class={s.infoBadgeTip}
				role="tooltip"
				data-visible={positioned() && (hovered() || focused()) ? "true" : undefined}
			>
				{props.children}
			</span>
		</span>
	);
};
