import { appLogger } from "../stores/appLogger";
import { toastsStore } from "../stores/toasts";
import type { subscribeEvents } from "../transport";

export interface SettingsChanged {
	section: string;
	generation: number;
	persisted: boolean;
	applied: boolean;
	__tuic_origin?: { connection: string };
}

interface Reader<T> {
	section: string;
	connectionId?: string;
	read: () => Promise<T>;
	/** Non-saving adoption: advance the baseline and keep unsaved intent. */
	apply: (snapshot: T) => void;
}

/** One disposable subscription per window, never a migration/bootstrap hydration. */
export function createSettingsRefreshCoordinator(subscribe?: typeof subscribeEvents) {
	const readers = new Set<{
		owner?: string;
		section: string;
		invalidate: () => void;
		cancel: () => void;
		dispose: () => void;
	}>();
	const generations = new Map<string, number>();
	const disconnected = new Set<string>();
	let unlisten: (() => void) | undefined;
	let starting: Promise<void> | undefined;
	let lifetime = 0;

	function resync(owner?: string) {
		for (const key of generations.keys()) if (key.startsWith(`${owner ?? ""}\0`)) generations.delete(key);
		for (const reader of readers) if (reader.owner === owner) reader.invalidate();
	}

	function start() {
		if (unlisten || starting) return;
		const current = lifetime;
		const subscribeNow: typeof subscribeEvents = (...args) =>
			subscribe ? subscribe(...args) : import("../transport").then((transport) => transport.subscribeEvents(...args));
		starting = subscribeNow(
			{
				"settings-changed": (raw) => {
					const event = raw as SettingsChanged;
					if (
						!event.persisted ||
						!Number.isSafeInteger(event.generation) ||
						event.generation < 1 ||
						typeof event.section !== "string"
					)
						return;
					const owner = event.__tuic_origin?.connection;
					if (![...readers].some((reader) => reader.owner === owner && reader.section === event.section)) return;
					const key = `${owner ?? ""}\0${event.section}`;
					if (event.generation <= (generations.get(key) ?? 0)) return;
					generations.set(key, event.generation);
					for (const reader of readers)
						if (reader.owner === owner && reader.section === event.section) reader.invalidate();
				},
				"remote-connection-status": (raw) => {
					const status = raw as { id: string; status: string };
					if (status.status === "connected") {
						disconnected.delete(status.id);
						resync(status.id);
					} else if (status.status === "disconnected") {
						disconnected.add(status.id);
						for (const reader of readers) if (reader.owner === status.id) reader.cancel();
					}
				},
			},
			{
				onResync: () => {
					generations.clear();
					for (const reader of readers) reader.invalidate();
				},
			},
		)
			.then((stop) => {
				if (current !== lifetime || readers.size === 0) stop();
				else {
					unlisten = stop;
					for (const reader of readers) reader.invalidate();
				}
			})
			.catch((error) => appLogger.warn("config", "Settings subscription failed", error))
			.finally(() => {
				starting = undefined;
				if (readers.size && current !== lifetime) start();
			});
	}

	return {
		register<T>(reader: Reader<T>): () => void {
			let disposed = false;
			let sequence = 0;
			let queued = false;
			let reading = false;
			let pending = false;
			const entry = {
				owner: reader.connectionId,
				section: reader.section,
				invalidate() {
					sequence++;
					pending = true;
					if (queued || reading || disposed) return;
					queued = true;
					queueMicrotask(() => {
						queued = false;
						if (disposed || (reader.connectionId && disconnected.has(reader.connectionId))) return;
						pending = false;
						reading = true;
						const current = sequence;
						void reader
							.read()
							.then((snapshot) => {
								if (!disposed && current === sequence) reader.apply(snapshot);
							})
							.catch((error) => {
								if (!disposed) appLogger.warn("config", "Settings reload failed", error);
							})
							.finally(() => {
								reading = false;
								if (pending && !disposed) entry.invalidate();
							});
					});
				},
				cancel() {
					sequence++;
				},
				dispose() {
					disposed = true;
					sequence++;
				},
			};
			readers.add(entry);
			start();
			if (unlisten) entry.invalidate();
			return () => {
				entry.dispose();
				readers.delete(entry);
				if (readers.size === 0) {
					lifetime++;
					unlisten?.();
					unlisten = undefined;
					generations.clear();
					disconnected.clear();
				}
			};
		},
	};
}

export const settingsRefresh = createSettingsRefreshCoordinator();

/** Content-free conflict notice: neither values nor private field paths enter the bell. */
export function reportSettingsConflicts(conflicts: readonly string[]): void {
	if (conflicts.length)
		toastsStore.add(
			"Settings changed elsewhere",
			"Your unsaved edits were kept. Review them before saving again.",
			"warn",
		);
}
