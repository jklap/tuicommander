import { type Component, For, Show } from "solid-js";
import { Input } from "../../ui/Input";
import s from "./fields.module.css";
import { SettingField, type SettingFieldProps } from "./SettingField";

export interface SettingListRow {
	id: string;
	values: Record<string, string>;
}

export interface SettingListProps extends SettingFieldProps {
	rows: readonly SettingListRow[];
	columns: readonly { id: string; label: string; placeholder?: string }[];
	onChange: (rows: SettingListRow[]) => void;
	/** The caller supplies a stable ID and default values for a new row. */
	onAdd?: () => SettingListRow;
}

export const SettingList: Component<SettingListProps> = (props) => (
	<SettingField label={props.label} hint={props.hint}>
		{(id, hintId) => (
			<div id={id} role="group" aria-label={props.label} aria-describedby={hintId} class={s.list}>
				<For each={props.rows.map((row) => row.id)}>
					{(rowId, index) => (
						<div class={s.row} role="group" aria-label={`Row ${rowId}`}>
							<For each={props.columns}>
								{(column) => (
									<label class={s.cell}>
										<span>{column.label}</span>
										<Input
											type="text"
											value={props.rows[index()]?.values[column.id] ?? ""}
											placeholder={column.placeholder}
											disabled={props.disabled}
											onInput={(e) => {
												const value = e.currentTarget.value;
												props.onChange(
													props.rows.map((current) =>
														current.id === rowId
															? { ...current, values: { ...current.values, [column.id]: value } }
															: current,
													),
												);
											}}
										/>
									</label>
								)}
							</For>
							<button
								type="button"
								class={s.button}
								disabled={props.disabled}
								aria-label={`Remove row ${rowId}`}
								onClick={() => props.onChange(props.rows.filter((current) => current.id !== rowId))}
							>
								Remove
							</button>
						</div>
					)}
				</For>
				<Show when={props.onAdd}>
					{(add) => (
						<button
							type="button"
							class={s.button}
							disabled={props.disabled}
							onClick={() => props.onChange([...props.rows, add()()])}
						>
							Add row
						</button>
					)}
				</Show>
			</div>
		)}
	</SettingField>
);
