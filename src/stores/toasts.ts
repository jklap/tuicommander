import { createStore } from "solid-js/store";
import { notificationManager } from "../notifications";
import { activityStore } from "./activityStore";
import { notificationsStore } from "./notifications";

export interface Toast {
	id: number;
	title: string;
	message: string;
	level: "info" | "warn" | "error";
	createdAt: number;
	repoPath?: string;
	/** Backend session that raised this toast. Set for MCP `ui action=toast`,
	 *  where it is what makes the toast clickable: a repo holds many tabs, so
	 *  only the session id can name the terminal that actually spoke. */
	sessionId?: string;
	action?: { label: string; onClick: () => void };
}

let nextId = 1;

/** Auto-dismiss delay per level (ms). `info` is transient; `warn` lingers so
 *  actionable messages can be read; `error` is sticky (0 = never auto-dismiss)
 *  and stays until the user clicks it away. Callers can override per toast. */
export const DEFAULT_DURATION_MS: Record<Toast["level"], number> = {
	info: 20000,
	warn: 60000,
	error: 0,
};

/** Route a toast's `sound: true` flag through the same customizable Info/
 *  Warning/Error sounds as everything else in Settings > Notifications —
 *  respecting the master toggle, per-sound toggle, volume, output device,
 *  and any preset/custom file chosen for that event — instead of a separate
 *  fixed synth. Deliberately calls `notificationManager` directly rather
 *  than `notificationsStore.play()`: the store wrapper also increments the
 *  dock badge and can fire an OS notification, both meant for the agent-
 *  attention events (Question/Completion), not routine UI toasts. */
function playSoundForLevel(level: Toast["level"]): void {
	if (level === "warn") {
		void notificationManager.playWarning();
	} else if (level === "error") {
		void notificationManager.playError();
	} else {
		void notificationManager.playInfo();
	}
}

/** Activity section that collects the mirrored toasts in the toolbar bell.
 *  Registered in App.tsx next to the other built-in sections. */
export const TOAST_ACTIVITY_SECTION_ID = "messages";

/** Bell icon per level. Compile-time constants — ActivityItem.icon is rendered
 *  via innerHTML and must never carry a runtime-built string. */
const LEVEL_ICONS: Record<Toast["level"], string> = {
	info: '<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0zm0 3.5a1 1 0 1 1 0 2 1 1 0 0 1 0-2zM6.75 7h1.5a.75.75 0 0 1 .75.75v3.5h.75a.75.75 0 0 1 0 1.5h-3a.75.75 0 0 1 0-1.5h.75V8.5h-.75a.75.75 0 0 1 0-1.5z"/></svg>',
	warn: '<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8.87 1.5a1 1 0 0 0-1.74 0L.36 13.25A1 1 0 0 0 1.23 14.75h13.54a1 1 0 0 0 .87-1.5zM8 5.25a.75.75 0 0 1 .75.75v3a.75.75 0 0 1-1.5 0V6a.75.75 0 0 1 .75-.75zm0 5.5a1 1 0 1 1 0 2 1 1 0 0 1 0-2z"/></svg>',
	error:
		'<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0zm3.36 10.3a.75.75 0 0 1-1.06 1.06L8 9.06l-2.3 2.3a.75.75 0 0 1-1.06-1.06L6.94 8 4.64 5.7a.75.75 0 0 1 1.06-1.06L8 6.94l2.3-2.3a.75.75 0 0 1 1.06 1.06L9.06 8l2.3 2.3z"/></svg>',
};

/** A toast auto-dismisses, often while the user is looking at another window, so
 *  the message is gone before it is read. Mirroring it into the bell keeps it
 *  readable afterwards. Opt out with the "Keep toasts in the bell" setting. */
function mirrorToBell(toast: Toast): void {
	if (!notificationsStore.state.config.toasts_in_bell) return;
	activityStore.addItem({
		id: `toast-${toast.id}`,
		pluginId: "core",
		sectionId: TOAST_ACTIVITY_SECTION_ID,
		title: toast.title,
		subtitle: toast.message || undefined,
		icon: LEVEL_ICONS[toast.level],
		severity: toast.level,
		dismissible: true,
		onClick: toast.action?.onClick,
		repoPath: toast.repoPath,
	});
}

function createToastsStore() {
	const [state, setState] = createStore<{ toasts: Toast[] }>({ toasts: [] });
	const dismissTimers = new Map<number, ReturnType<typeof setTimeout>>();

	return {
		get toasts() {
			return state.toasts;
		},

		/** Whether an identical toast is already visible. Duplicate backend events
		 * must not create a stack of copies, while distinct errors still stack. */
		hasVisible(title: string, message: string, level: Toast["level"], repoPath?: string): boolean {
			return state.toasts.some(
				(toast) =>
					toast.title === title && toast.message === message && toast.level === level && toast.repoPath === repoPath,
			);
		},

		add(
			title: string,
			message = "",
			level: "info" | "warn" | "error" = "info",
			sound = false,
			action?: { label: string; onClick: () => void },
			durationMs?: number,
			repoPath?: string,
			sessionId?: string,
		) {
			if (this.hasVisible(title, message, level, repoPath)) {
				return -1;
			}
			const id = nextId++;
			const toast: Toast = { id, title, message, level, createdAt: Date.now(), action, repoPath, sessionId };
			setState("toasts", (prev) => [...prev, toast]);
			mirrorToBell(toast);
			if (sound) playSoundForLevel(level);
			// A non-positive duration means "sticky" — no auto-dismiss timer, so the
			// toast stays until the user clicks it away.
			const timeout = durationMs ?? DEFAULT_DURATION_MS[level];
			if (timeout > 0) {
				dismissTimers.set(
					id,
					setTimeout(() => this.remove(id), timeout),
				);
			}
			return id;
		},

		remove(id: number) {
			const timer = dismissTimers.get(id);
			if (timer !== undefined) {
				clearTimeout(timer);
				dismissTimers.delete(id);
			}
			setState("toasts", (prev) => prev.filter((t) => t.id !== id));
		},
	};
}

export const toastsStore = createToastsStore();
