import { settingsCatalogue } from "../generated/settingsCatalogue";

// The catalogue lookups that need no message catalog. `stores/settings.ts` is reachable from the
// mobile entry, which must never load `i18n/t.ts` and its 20 KB-gzip en.json (see
// `mobile/__tests__/mobileEagerGraph.test.ts`), so the label/hint helpers that call `t()` live in
// `settingsCatalogue.ts` and only this file is imported by eager code.

export type SettingId = keyof typeof settingsCatalogue.fields;
export interface SettingDescriptor {
	id: string;
	labelKey: string;
	helpKey: string;
	label: string;
	schema: {
		[key: string]: unknown;
		description?: string;
		minimum?: number;
		maximum?: number;
		enum?: readonly unknown[];
	};
	effect: string;
	default?: unknown;
}

/** Rust owns the identity and semantics; pages retain their established presentation. */
export function settingDescriptor(id: SettingId): SettingDescriptor {
	return settingsCatalogue.fields[id];
}
export function settingNumberDefault(id: SettingId): number {
	const value = settingDescriptor(id).default;
	if (typeof value !== "number") throw new Error(`Setting ${id} has no numeric default`);
	return value;
}
