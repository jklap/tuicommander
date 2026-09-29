import { listen } from "../invoke";
import { progressStore } from "../stores/progress";
import { navigateToTerminal } from "../utils/navigateToTerminal";

export type NativeNoticeTarget =
	| { kind: "terminal"; id: string }
	| { kind: "progress"; project: string; ptyId?: string | null };

export function navigateFromNativeNotice(target: NativeNoticeTarget): void {
	if (target.kind === "terminal") navigateToTerminal(target.id);
	else progressStore.open(target.project, target.ptyId ?? null);
}

export function listenForNativeNoticeClicks(): Promise<() => void> {
	return listen<NativeNoticeTarget>("native-notification-click", (event) => navigateFromNativeNotice(event.payload));
}
