import { createEffect, createMemo, createSignal, For, Match, on, Show, Switch } from "solid-js";
import type { WorkItemFieldSchema, WorkItemFieldValue } from "../../../types/work-items.gen";
import { TaskMarkdown } from "./TaskMarkdown";
import styles from "./TasksDetail.module.css";

export function TaskField(props: {
	schema: WorkItemFieldSchema;
	value: WorkItemFieldValue | null;
	disabled: boolean;
	canEdit: boolean;
	onSave: (value: WorkItemFieldValue | null) => Promise<boolean>;
}) {
	const [text, setText] = createSignal("");
	const [selected, setSelected] = createSignal<string[]>([]);
	const [dirty, setDirty] = createSignal(false);
	createEffect(
		on(
			() => JSON.stringify(props.value),
			() => {
				const value = props.value;
				setText(value && value.kind !== "user" && value.kind !== "labels" ? String(value.value) : "");
				setSelected(value?.kind === "user" || value?.kind === "labels" ? value.value.map((entry) => entry.id) : []);
				setDirty(false);
			},
		),
	);
	const editable = () => props.canEdit && props.schema.editable;
	const options = createMemo(() => {
		const choices = new Map(props.schema.options.map((option) => [option.id, option]));
		if (props.value?.kind === "user" || props.value?.kind === "labels") {
			for (const entry of props.value.value)
				if (!choices.has(entry.id)) choices.set(entry.id, { id: entry.id, name: entry.name });
		}
		return [...choices.values()];
	});
	const valid = () => {
		if (
			props.schema.required &&
			(props.schema.kind === "user" || props.schema.kind === "labels" ? selected().length === 0 : !text().trim())
		)
			return false;
		return props.schema.kind !== "number" || !text().trim() || Number.isFinite(Number(text()));
	};
	async function save(event: SubmitEvent) {
		event.preventDefault();
		if (!valid() || props.disabled || !editable()) return;
		let value: WorkItemFieldValue | null = null;
		const kind = props.schema.kind;
		if (kind === "labels")
			value = {
				kind,
				value: selected().map((id) => ({ id, name: options().find((option) => option.id === id)?.name ?? id })),
			};
		else if (kind === "user")
			value = {
				kind,
				value: selected().map(
					(id) =>
						(props.value?.kind === "user" ? props.value.value.find((user) => user.id === id) : undefined) ?? {
							id,
							name: options().find((option) => option.id === id)?.name ?? id,
							avatarUrl: null,
						},
				),
			};
		else if (text() !== "") value = kind === "number" ? { kind, value: Number(text()) } : { kind, value: text() };
		if (await props.onSave(value)) setDirty(false);
	}
	const input = (value: string) => {
		setText(value);
		setDirty(true);
	};
	return (
		<form class={styles.field} onSubmit={save}>
			<label class={styles.fieldLabel}>
				<span>
					{props.schema.name}
					{props.schema.required ? " *" : ""}
				</span>
				<Show
					when={editable()}
					fallback={
						<Switch fallback={<span>{text() || "Not set"}</span>}>
							<Match when={props.schema.kind === "markdown"}>
								<TaskMarkdown content={text()} />
							</Match>
							<Match when={props.schema.kind === "user" || props.schema.kind === "labels"}>
								<span>
									{props.value?.kind === "user" || props.value?.kind === "labels"
										? props.value.value.map((entry) => entry.name).join(", ") || "Not set"
										: "Not set"}
								</span>
							</Match>
						</Switch>
					}
				>
					<Switch
						fallback={
							<input
								aria-label={props.schema.name}
								value={text()}
								disabled={props.disabled}
								required={props.schema.required}
								type={
									props.schema.kind === "number"
										? "number"
										: props.schema.kind === "date"
											? "date"
											: props.schema.kind === "link"
												? "url"
												: "text"
								}
								step={props.schema.kind === "number" ? "any" : undefined}
								onInput={(event) => input(event.currentTarget.value)}
							/>
						}
					>
						<Match when={props.schema.kind === "markdown"}>
							<textarea
								aria-label={props.schema.name}
								value={text()}
								disabled={props.disabled}
								required={props.schema.required}
								onInput={(event) => input(event.currentTarget.value)}
							/>
						</Match>
						<Match when={props.schema.kind === "enum"}>
							<select
								aria-label={props.schema.name}
								disabled={props.disabled}
								required={props.schema.required}
								onChange={(event) => input(event.currentTarget.value)}
							>
								<option value="" selected={text() === ""}>
									Not set
								</option>
								<For each={props.schema.options}>
									{(option) => (
										<option value={option.id} selected={text() === option.id}>
											{option.name}
										</option>
									)}
								</For>
							</select>
						</Match>
						<Match when={props.schema.kind === "user" || props.schema.kind === "labels"}>
							<select
								aria-label={props.schema.name}
								multiple
								disabled={props.disabled}
								required={props.schema.required}
								onChange={(event) => {
									setSelected(Array.from(event.currentTarget.selectedOptions, (option) => option.value));
									setDirty(true);
								}}
							>
								<For each={options()}>
									{(option) => (
										<option value={option.id} selected={selected().includes(option.id)}>
											{option.name}
										</option>
									)}
								</For>
							</select>
						</Match>
					</Switch>
				</Show>
			</label>
			<Show when={editable()}>
				<button
					type="submit"
					disabled={props.disabled || !dirty() || !valid()}
					aria-label={`Save ${props.schema.name}`}
				>
					Save
				</button>
			</Show>
		</form>
	);
}
