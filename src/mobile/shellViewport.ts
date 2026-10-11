export interface ShellViewport {
	top: string;
	height: string;
	bottomInset: string;
}

/** Less than this covered is a browser-chrome or status-bar inset, not a soft keyboard. */
const KEYBOARD_MIN_PX = 120;

/**
 * Where the fixed shell sits. With no keyboard it takes `auto` height between
 * top 0 and bottom 0, so it reaches the physical bottom edge: on an iOS
 * standalone PWA `visualViewport.height` is the layout height minus the top
 * inset, and a shell sized by it leaves a band under the tabs. Above a keyboard
 * the shell follows `visualViewport`, and the home indicator is not there.
 */
export function shellViewport(vv: { height: number; offsetTop: number }, layoutHeight: number): ShellViewport {
	if (layoutHeight - vv.height < KEYBOARD_MIN_PX) {
		return { top: "0px", height: "auto", bottomInset: "env(safe-area-inset-bottom, 0px)" };
	}
	return { top: `${vv.offsetTop}px`, height: `${vv.height}px`, bottomInset: "0px" };
}
