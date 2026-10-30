import catalogueExclusions from "../components/SettingsPanel/settingsCatalogueExclusions.json";
import { t } from "../i18n";
import { type SettingId, settingDescriptor } from "./settingsCatalogueCore";

export type { SettingDescriptor, SettingId } from "./settingsCatalogueCore";
export { settingDescriptor, settingNumberDefault } from "./settingsCatalogueCore";

export function settingLabel(id: SettingId | undefined, fallback: string): string {
	return id ? t(settingDescriptor(id).labelKey, fallback) : fallback;
}
export function settingHint(id: SettingId | undefined, fallback: string | undefined): string | undefined {
	return id && fallback ? t(settingDescriptor(id).helpKey, fallback) : fallback;
}

/** Preserve human option labels, but take the admissible values from the owner schema. */
export function settingOptions(
	id: SettingId,
	labels: Readonly<Record<string, string>> = {},
	schemaPath: readonly string[] = [],
): { value: string; label: string }[] {
	let schema: unknown = settingDescriptor(id).schema;
	for (const segment of schemaPath)
		schema = schema && typeof schema === "object" ? (schema as Record<string, unknown>)[segment] : undefined;
	const values = (schema as { enum?: readonly unknown[] } | undefined)?.enum;
	if (!values) throw new Error(`Setting ${id} does not have static choices`);
	return values.map((value) => {
		const encoded = value === null ? "" : String(value);
		return { value: encoded, label: labels[encoded] ?? (value === null ? "Default" : String(value)) };
	});
}

interface CatalogueEditorField {
	id: string;
	sensitivity: string;
	operations: readonly string[];
	schema: Record<string, unknown>;
}
/** Complex domain editors and credentials retain their existing typed owner workflows. */
export function hasCatalogueEditor(field: CatalogueEditorField): boolean {
	if (Object.hasOwn(catalogueExclusions, field.id)) return false;
	if (
		field.sensitivity === "secret" ||
		field.sensitivity === "derived_secret" ||
		!(field.operations as readonly string[]).includes("set")
	)
		return false;
	const schema = field.schema as Record<string, unknown>;
	const types = Array.isArray(schema.type) ? schema.type : [schema.type];
	return (
		types.every((type) => ["null", "string", "integer", "number", "boolean", "array"].includes(String(type))) &&
		(!types.includes("array") || (schema.items as { type?: string } | undefined)?.type === "string")
	);
}
