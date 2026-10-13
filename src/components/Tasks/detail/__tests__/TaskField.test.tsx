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

	// Catches: required marker drawn from schema.required while editable is false
	it("read_only_field_renders_without_a_required_star", () => {
		const locked = render(() => (
			<TaskField
				schema={{ id: "title", name: "Title", kind: "text", required: true, editable: false, options: [] }}
				value={{ kind: "text", value: "Fixed" }}
				disabled={false}
				canEdit
				onSave={async () => true}
			/>
		));
		expect(locked.container.textContent).toContain("Title");
		expect(locked.container.textContent).not.toContain("*");
	});

	// Catches: a read-only field because the detail has no edit_field action still draws the star
	it("field_without_an_edit_action_renders_without_a_required_star", () => {
		const { container } = render(() => (
			<TaskField
				schema={{ id: "title", name: "Title", kind: "text", required: true, editable: true, options: [] }}
				value={{ kind: "text", value: "Fixed" }}
				disabled={false}
				canEdit={false}
				onSave={async () => true}
			/>
		));
		expect(container.textContent).not.toContain("*");
	});

	// Catches: the star dropped from a field the user can edit and must not clear
	it("editable_required_field_keeps_its_required_star", () => {
		const { container } = render(() => (
			<TaskField
				schema={{ id: "title", name: "Title", kind: "text", required: true, editable: true, options: [] }}
				value={{ kind: "text", value: "Editable" }}
				disabled={false}
				canEdit
				onSave={async () => true}
			/>
		));
		expect(container.textContent).toContain("Title *");
	});
});
