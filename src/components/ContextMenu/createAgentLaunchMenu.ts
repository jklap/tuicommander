import { createSignal, onCleanup } from "solid-js";
import type { ContextMenuItem } from "./ContextMenu";
import { createContextMenu } from "./ContextMenu";

export const AGENT_LAUNCH_LONG_PRESS_MS = 500;

/** The agent builder nests every agent under one "Add Agent" entry; under a + the list is the menu. */
function flattenAgentItems(items: ContextMenuItem[]): ContextMenuItem[] {
	return items.length === 1 && items[0].children ? items[0].children : items;
}

/**
 * Long press or right click on a "+" button lists the agents, so a new
 * terminal can open straight into one instead of a shell. A plain click keeps
 * opening a plain terminal; a press that opened the menu must not also do that.
 */
export function createAgentLaunchMenu(getItems: () => ContextMenuItem[]) {
	const menu = createContextMenu();
	const [items, setItems] = createSignal<ContextMenuItem[]>([]);
	let pressTimer: ReturnType<typeof setTimeout> | undefined;
	let pressFired = false;

	const openBelow = (btn: HTMLElement): boolean => {
		const list = flattenAgentItems(getItems());
		if (list.length === 0) return false;
		setItems(list);
		const rect = btn.getBoundingClientRect();
		menu.openAt(rect.left, rect.bottom + 4);
		return true;
	};

	const cancelPress = () => {
		clearTimeout(pressTimer);
		pressTimer = undefined;
	};

	const onPointerDown = (e: PointerEvent) => {
		pressFired = false;
		if (e.button !== 0) return;
		const btn = e.currentTarget as HTMLElement;
		pressTimer = setTimeout(() => {
			pressTimer = undefined;
			pressFired = openBelow(btn);
		}, AGENT_LAUNCH_LONG_PRESS_MS);
	};

	const onContextMenu = (e: MouseEvent) => {
		e.preventDefault();
		// A touch long press also fires the native contextmenu. A right click
		// never starts the timer (button 0 only), so a pending or fired press
		// means the long press already owns this event.
		if (pressTimer !== undefined || pressFired || openBelow(e.currentTarget as HTMLElement)) {
			e.stopPropagation();
		}
	};

	/** True when the click ends a press that opened the menu and must be swallowed. */
	const consumeClick = (): boolean => {
		if (!pressFired) return false;
		pressFired = false;
		return true;
	};

	const close = () => {
		// A release off the button fires no click, so the flag would otherwise
		// swallow the next keyboard activation of +.
		pressFired = false;
		menu.close();
	};

	onCleanup(cancelPress);

	return {
		items,
		position: menu.position,
		visible: menu.visible,
		close,
		consumeClick,
		buttonHandlers: {
			onPointerDown,
			onPointerUp: cancelPress,
			onPointerLeave: cancelPress,
			onContextMenu,
		},
	};
}
