import { type Component, createEffect, createResource, createSignal, For, onCleanup, Show } from "solid-js";
import { invoke } from "../../invoke";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { SettingField } from "./fields/SettingField";
import { type PluginSettingsField, type PluginSettingsPage, pluginFieldLabel } from "./pluginSettingsCatalogue";
import { SettingList, SettingMultiSelect, SettingStatus } from "./SettingFields";
import s from "./Settings.module.css";

interface Snapshot {
	revision: string;
	values: Record<string, { effective: unknown }>;
}

/** Persistence and validation remain in the existing scoped Rust settings service. */
export const PluginSettingsTab: Component<{ page: PluginSettingsPage }> = (props) => {
	const [snapshot, { refetch }] = createResource(
		() => props.page,
		(page) =>
			invoke<Snapshot>("plugin_settings_action", {
				pluginId: page.id,
				action: { action: "get", paths: page.fields.map((field) => field.path) },
			}),
	);
	const [busy, setBusy] = createSignal(false);
	const [message, setMessage] = createSignal("");
	let disposed = false;
	onCleanup(() => {
		disposed = true;
	});

	async function write(field: PluginSettingsField, value: unknown, reset = false) {
		const current = snapshot();
		if (disposed || busy() || !current) return;
		setBusy(true);
		setMessage("");
		try {
			const reply = await invoke<{ effects: string[]; restart: string[] }>("plugin_settings_action", {
				pluginId: props.page.id,
				action: reset
					? { action: "reset", path: field.path, expectedRevision: current.revision }
					: { action: "set", path: field.path, value, expectedRevision: current.revision },
			});
			if (!disposed) setMessage([...reply.effects, ...reply.restart].join(" · "));
		} catch {
			// Do not render backend text: it may describe private settings.
			if (!disposed) setMessage("Setting was not saved. Reloaded the latest values; try again.");
		} finally {
			if (!disposed) {
				try {
					await refetch();
				} catch {
					// The resource error renders the local read-error alert.
				} finally {
					setBusy(false);
				}
			}
		}
	}

	return (
		<div class={s.section}>
			<h3>{props.page.label}</h3>
			<Show when={snapshot.error}>
				<p role="alert" class={s.hint}>
					Could not read plugin settings.
				</p>
			</Show>
			<Show when={message()}>
				<p role="status" class={s.hint}>
					{message()}
				</p>
			</Show>
			<Show when={!snapshot.error && snapshot()}>
				{(current) => (
					<For each={props.page.fields}>
						{(field) => (
							<>
								<PluginField
									field={field}
									value={current().values[field.path]?.effective}
									disabled={busy() || snapshot.loading}
									onWrite={(value) => void write(field, value)}
								/>
								<Show when={field.operations.includes("reset")}>
									<div class={s.actions}>
										<button disabled={busy() || snapshot.loading} onClick={() => void write(field, undefined, true)}>
											Reset {pluginFieldLabel(field)}
										</button>
									</div>
								</Show>
							</>
						)}
					</For>
				)}
			</Show>
		</div>
	);
};

const PluginField: Component<{
	field: PluginSettingsField;
	value: unknown;
	disabled: boolean;
	onWrite: (value: unknown) => void;
}> = (props) => {
	const label = () => pluginFieldLabel(props.field);
	const hint = () => props.field.schema.description;
	const secret = () => props.field.sensitivity === "secret";
	const writable = () => props.field.operations.includes("set");
	const disabled = () => props.disabled || !writable();
	const [draft, setDraft] = createSignal("");
	const [invalid, setInvalid] = createSignal(false);
	const [dirty, setDirty] = createSignal(false);
	createEffect(() => {
		setDraft(
			secret()
				? ""
				: props.field.schema.type === "array"
					? JSON.stringify(props.value ?? [])
					: String(props.value ?? ""),
		);
		setInvalid(false);
		setDirty(false);
	});
	function commit() {
		if (disabled() || !dirty()) return;
		let value: unknown = draft();
		try {
			if (props.field.schema.type === "array") value = JSON.parse(draft());
			if (props.field.schema.type === "number" || props.field.schema.type === "integer") {
				if (!draft().trim() || !Number.isFinite(Number(draft()))) throw new Error("invalid number");
				value = Number(draft());
			}
			setInvalid(false);
			if (secret() || JSON.stringify(value) !== JSON.stringify(props.value)) props.onWrite(value);
		} catch {
			setInvalid(true);
		}
	}
	const choices = () => props.field.schema.enum;
	const arrayChoices = () => props.field.schema.items?.enum;
	const stringList = () => props.field.schema.type === "array" && props.field.schema.items?.type === "string";

	return (
		<Show
			when={writable()}
			fallback={
				<SettingStatus
					label={label()}
					hint={hint()}
					value={
						secret() ? String((props.value as { status?: string })?.status ?? "unset") : JSON.stringify(props.value)
					}
				/>
			}
		>
			<Show
				when={stringList()}
				fallback={
					<SettingField label={label()} hint={hint()}>
						{(id, hintId) => (
							<>
								<Show
									when={props.field.schema.type === "boolean"}
									fallback={
										<Show
											when={choices()}
											fallback={
												<Input
													id={id}
													aria-describedby={hintId}
													type={
														secret()
															? "password"
															: props.field.schema.type === "number" || props.field.schema.type === "integer"
																? "number"
																: "text"
													}
													value={draft()}
													disabled={disabled()}
													placeholder={
														secret()
															? String((props.value as { status?: string })?.status ?? "unset")
															: props.field.schema.type === "array"
																? "JSON array"
																: undefined
													}
													onInput={(event) => {
														setDraft(event.currentTarget.value);
														setDirty(true);
													}}
													onBlur={commit}
												/>
											}
										>
											{(values) => (
												<Select
													id={id}
													aria-describedby={hintId}
													value={JSON.stringify(props.value)}
													disabled={disabled()}
													onChange={(event) => props.onWrite(JSON.parse(event.currentTarget.value))}
												>
													<For each={values()}>
														{(value) => <option value={JSON.stringify(value)}>{String(value)}</option>}
													</For>
												</Select>
											)}
										</Show>
									}
								>
									<input
										id={id}
										aria-describedby={hintId}
										type="checkbox"
										checked={props.value === true}
										disabled={disabled()}
										onChange={(event) => props.onWrite(event.currentTarget.checked)}
									/>
								</Show>
								<Show when={invalid()}>
									<p role="alert" class={s.hint}>
										Enter a valid {props.field.schema.type === "array" ? "JSON array" : "number"}.
									</p>
								</Show>
							</>
						)}
					</SettingField>
				}
			>
				<Show
					when={arrayChoices()?.every((value) => typeof value === "string") && arrayChoices()}
					fallback={
						<div
							onFocusOut={(event) => {
								if (!event.currentTarget.contains(event.relatedTarget as Node | null)) commit();
							}}
						>
							<SettingList
								label={label()}
								hint={hint()}
								disabled={disabled()}
								rows={(JSON.parse(draft() || "[]") as string[]).map((value, index) => ({
									id: String(index),
									values: { value },
								}))}
								columns={[{ id: "value", label: "Value" }]}
								onChange={(rows) => {
									setDraft(JSON.stringify(rows.map((row) => row.values.value)));
									setDirty(true);
								}}
								onAdd={() => ({ id: String((JSON.parse(draft() || "[]") as string[]).length), values: { value: "" } })}
							/>
						</div>
					}
				>
					{(values) => (
						<SettingMultiSelect
							label={label()}
							hint={hint()}
							disabled={disabled()}
							value={(props.value ?? []) as string[]}
							options={(values() as string[]).map((value) => ({ value, label: value }))}
							onInput={props.onWrite}
						/>
					)}
				</Show>
			</Show>
		</Show>
	);
};
