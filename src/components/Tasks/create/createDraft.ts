import type { WorkItemFieldSchema, WorkItemFieldValue } from "../../../types/work-items.gen";

/** Untyped `createFields()` declares the creatable item types as an enum field with this ID. */
export const ITEM_TYPE_FIELD = "itemType";

/** Raw control state: option IDs for user/labels, text for every other kind. */
export type DraftValue = string | string[];
export type Draft = Record<string, DraftValue>;

const isMulti = (schema: WorkItemFieldSchema) => schema.kind === "user" || schema.kind === "labels";

export const emptyValue = (schema: WorkItemFieldSchema): DraftValue => (isMulti(schema) ? [] : "");

/** Fields the form renders for a type schema: editable ones, never the type picker itself. */
export const formFields = (schema: readonly WorkItemFieldSchema[]) =>
	schema.filter((field) => field.editable && field.id !== ITEM_TYPE_FIELD);

/** Keep a value across a type change only where the new type has the same field with the same kind. */
export function carryOver(
	previous: readonly WorkItemFieldSchema[],
	next: readonly WorkItemFieldSchema[],
	draft: Draft,
) {
	const kinds = new Map(previous.map((field) => [field.id, field.kind]));
	return Object.fromEntries(
		next.map((field) => [
			field.id,
			kinds.get(field.id) === field.kind && field.id in draft ? draft[field.id] : emptyValue(field),
		]),
	);
}

/** Converts a raw control value; `undefined` means the value is invalid and cannot be sent. */
export function toFieldValue(
	schema: WorkItemFieldSchema,
	raw: DraftValue | undefined,
): WorkItemFieldValue | null | undefined {
	if (schema.kind === "user" || schema.kind === "labels") {
		const ids = Array.isArray(raw) ? raw : [];
		if (ids.length === 0) return null;
		const name = (id: string) => schema.options.find((option) => option.id === id)?.name ?? id;
		return schema.kind === "user"
			? { kind: "user", value: ids.map((id) => ({ id, name: name(id), avatarUrl: null })) }
			: { kind: "labels", value: ids.map((id) => ({ id, name: name(id) })) };
	}
	const text = typeof raw === "string" ? raw : "";
	if (!text.trim()) return null;
	if (schema.kind === "number") {
		const value = Number(text);
		return Number.isFinite(value) ? { kind: "number", value } : undefined;
	}
	if (schema.kind === "enum" && !schema.options.some((option) => option.id === text)) return undefined;
	return { kind: schema.kind, value: text };
}

/** Field values for `WorkItemCreate`, or the name of the first field that blocks submission. */
export type Collected =
	| { ok: true; fields: Record<string, WorkItemFieldValue | null> }
	| { ok: false; blocker: string };

export function collectFields(schema: readonly WorkItemFieldSchema[], draft: Draft): Collected {
	const fields: Record<string, WorkItemFieldValue | null> = {};
	for (const field of schema) {
		const value = toFieldValue(field, draft[field.id]);
		if (value === undefined || (value === null && field.required)) return { ok: false, blocker: field.name };
		fields[field.id] = value;
	}
	return { ok: true, fields };
}
