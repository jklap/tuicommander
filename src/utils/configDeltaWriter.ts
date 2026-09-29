import { invoke } from "../invoke";

/** Retain each loaded snapshot so writes carry only this client's changes. */
export function createConfigDeltaWriter<T>(command: string, field: "config" | "layout" | "items" = "config") {
	let base: T | undefined;
	let tail: Promise<void> | null = null;
	const snapshot = (value: T): T => JSON.parse(JSON.stringify(value)) as T;
	return {
		loaded(value: T): void {
			base = snapshot(value);
		},
		save(value: T): Promise<void> {
			const desired = snapshot(value);
			const write = async () => {
				if (base === undefined) throw new Error(`${command} refused before config load`);
				await invoke(command, { base, [field]: desired });
				base = desired;
			};
			const operation = tail ? tail.then(write) : write();
			const settled = operation.then(() => undefined, () => undefined);
			tail = settled;
			void settled.then(() => {
				if (tail === settled) tail = null;
			});
			return operation;
		},
	};
}
