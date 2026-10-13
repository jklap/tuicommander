// @vitest-environment jsdom
import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import type { WorkItemFieldKind, WorkItemFieldValue } from "../../../../types/work-items.gen";
import { TaskField } from "../TaskField";

describe("TaskField", () => {
	// Catches: coercing all scalar edits to text drops the field kind or corrupts numeric zero.
	it.each<[WorkItemFieldKind, string, WorkItemFieldValue]>([
		["text", "New title", { kind: "text", value: "New title" }],
		["markdown", "**New body**", { kind: "markdown", value: "**New body**" }],
		["date", "2026-10-20", { kind: "date", value: "2026-10-20" }],
		["number", "0", { kind: "number", value: 0 }],
		["link", "https://example.com/item", { kind: "link", value: "https://example.com/item" }],
	])("%s editing preserves the typed value", async (kind, input, expected) => {
		let saved: WorkItemFieldValue | null = null;
		render(() => (
			<TaskField
				schema={{
					id: "field",
					name: "Field",
					kind,
					required: true,
					editable: true,
					sortable: false,
					groupable: false,
					filterable: false,
					options: [],
				}}
				value={null}
				disabled={false}
				canEdit
				onSave={async (value) => {
					saved = value;
					return true;
				}}
			/>
		));
		fireEvent.input(screen.getByLabelText("Field"), { target: { value: input } });
		fireEvent.click(screen.getByRole("button", { name: "Save Field" }));
		await waitFor(() => expect(saved).toEqual(expected));
	});

	// Catches: a required empty field submits null or a read-only field exposes mutation controls.
	it("required_empty_and_read_only_fields_cannot_be_saved", () => {
		render(() => (
			<TaskField
				schema={{
					id: "field",
					name: "Field",
					kind: "text",
					required: true,
					editable: true,
					sortable: false,
					groupable: false,
					filterable: false,
					options: [],
				}}
				value={{ kind: "text", value: "Original" }}
				disabled={false}
				canEdit
				onSave={async () => true}
			/>
		));
		fireEvent.input(screen.getByLabelText("Field"), { target: { value: "" } });
		expect(screen.getByRole("button", { name: "Save Field" })).toBeDisabled();
		const readonly = render(() => (
			<TaskField
				schema={{
					id: "locked",
					name: "Locked",
					kind: "text",
					required: false,
					editable: false,
					sortable: false,
					groupable: false,
					filterable: false,
					options: [],
				}}
				value={{ kind: "text", value: "Read only" }}
				disabled={false}
				canEdit
				onSave={async () => true}
			/>
		));
		expect(readonly.container.textContent).toContain("Read only");
		expect(readonly.container.querySelector("button, input")).toBeNull();
	});
});
