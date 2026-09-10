import { onCleanup, onMount } from "solid-js";
import { subscribePtyOpenUrl } from "../../stores/ptyOpenUrl";

/**
 * No UI of its own — just keeps the OSC 1337 `OpenURL` subscription alive.
 * Mounted by both shells (desktop and mobile), same as `McpConfirmHost`,
 * since the confirmation this reacts to can be answered from either.
 */
export function PtyOpenUrlHost() {
	onMount(() => {
		const unsubscribe = subscribePtyOpenUrl();
		onCleanup(() => {
			unsubscribe.then((fn) => fn()).catch(() => {});
		});
	});

	return null;
}

export default PtyOpenUrlHost;
