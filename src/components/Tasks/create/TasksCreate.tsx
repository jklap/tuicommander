import { For, Show } from "solid-js";
import { SettingField } from "../../SettingsPanel/SettingFields";
import { TaskCreateField } from "./TaskCreateField";
import styles from "./TasksCreate.module.css";
import { type TasksCreateOptions, useTasksCreate } from "./useTasksCreate";

export interface TasksCreateProps extends TasksCreateOptions {
	onCancel?: () => void;
}

/** Core-owned create form. The provider supplies item types and field schemas, never markup. */
export function TasksCreate(props: TasksCreateProps) {
	const model = useTasksCreate(props);
	const busy = () => model.loading() || model.submitting();
	return (
		<form
			class={styles.create}
			aria-label="Create task"
			onSubmit={(event) => {
				event.preventDefault();
				void model.submit();
			}}
		>
			<h2>New task in {props.provider.name}</h2>
			<SettingField label="Type *">
				{(id) => (
					<select
						id={id}
						aria-required="true"
						disabled={model.submitting() || model.types().length < 2}
						onChange={(event) => model.chooseType(event.currentTarget.value)}
					>
						<option value="" selected={model.itemType() === ""}>
							Choose a type
						</option>
						<For each={model.types()}>
							{(type) => (
								<option value={type.id} selected={model.itemType() === type.id}>
									{type.name}
								</option>
							)}
						</For>
					</select>
				)}
			</SettingField>
			<SettingField label="Title *">
				{(id) => (
					<input
						id={id}
						aria-required="true"
						value={model.title()}
						disabled={model.submitting()}
						onInput={(event) => model.setTitle(event.currentTarget.value)}
					/>
				)}
			</SettingField>
			<SettingField label="Description">
				{(id) => (
					<textarea
						id={id}
						value={model.body()}
						disabled={model.submitting()}
						onInput={(event) => model.setBody(event.currentTarget.value)}
					/>
				)}
			</SettingField>
			<div class={styles.fields}>
				<For each={model.schema()}>
					{(field) => (
						<TaskCreateField
							schema={field}
							value={model.draft()[field.id]}
							disabled={busy()}
							onInput={(value) => model.setValue(field.id, value)}
						/>
					)}
				</For>
			</div>
			<Show when={model.loading()}>
				<p class={styles.muted} role="status">
					Loading fields…
				</p>
			</Show>
			<Show when={model.error()}>
				{(error) => (
					<p class={styles.error} role="alert">
						{error()}
						<button type="button" onClick={model.retry} disabled={busy()}>
							Retry
						</button>
					</p>
				)}
			</Show>
			<div class={styles.actions}>
				<Show when={!busy() && model.blocker()}>
					{(field) => <span class={styles.muted}>Required: {field()}</span>}
				</Show>
				<Show when={props.onCancel}>
					<button type="button" onClick={props.onCancel} disabled={model.submitting()}>
						Cancel
					</button>
				</Show>
				<button type="submit" class={styles.primary} disabled={!model.canSubmit()}>
					{model.submitting() ? "Creating…" : "Create"}
				</button>
			</div>
		</form>
	);
}
