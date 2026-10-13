// @vitest-environment jsdom
import { fireEvent, render, screen } from "@solidjs/testing-library";
import { expect, it } from "vitest";
import type { WorkItemFieldSchema } from "../../../../types/work-items.gen";
import { formFields } from "../createDraft";
import { TaskCreateOptional } from "../TaskCreateOptional";
import { TasksCreate } from "../TasksCreate";
import { jiraFixtureProvider, WIDE_LAN_TYPE } from "./jiraFixtureProvider";

async function openWideForm() {
	const { provider } = jiraFixtureProvider("LAN_WIDE");
	const schema = formFields(await provider.createFields(WIDE_LAN_TYPE));
	render(() => <TasksCreate provider={provider} onCreated={() => {}} />);
	const picker = await screen.findByLabelText("Type *");
	await screen.findByRole("option", { name: "Task" });
	fireEvent.change(picker, { target: { value: WIDE_LAN_TYPE } });
	await screen.findByLabelText("Reporter *");
	const group = screen.getByText(/^Optional fields/).closest("details");
	if (!group) throw new Error("The optional fields have no group");
	return { schema, group };
}

// Catches: 47-56 optional controls rendered inline, so the required fields drown in a long form.
it("wide_lan_schema_lists_required_fields_first_and_folds_the_optional_ones", async () => {
	const { schema, group } = await openWideForm();
	const reporter = screen.getByLabelText("Reporter *");
	expect(group.open).toBe(false);
	expect(group.contains(reporter)).toBe(false);
	expect(reporter.compareDocumentPosition(group) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
	for (const field of schema.filter((entry) => !entry.required))
		expect(group.contains(screen.getAllByLabelText(field.name)[0]), field.name).toBe(true);
	expect(group).toHaveTextContent(`Optional fields (${schema.filter((entry) => !entry.required).length})`);
});

// Catches: the filter drops the fields it hides, or matches case-sensitively, so a field is lost.
it("optional_filter_hides_non_matches_without_removing_the_fields", async () => {
	const { schema } = await openWideForm();
	const wrapper = (name: string) => screen.getByLabelText(name).closest("[hidden]");
	fireEvent.input(screen.getByLabelText("Filter optional fields"), { target: { value: "story" } });
	expect(wrapper("Story Points")).toBeNull();
	expect(wrapper("Story point estimate")).toBeNull();
	expect(wrapper("Domain")).not.toBeNull();
	for (const field of schema.filter((entry) => !entry.required))
		expect(screen.getAllByLabelText(field.name)).toHaveLength(1);
	fireEvent.input(screen.getByLabelText("Filter optional fields"), { target: { value: "no such field" } });
	expect(screen.getByText("No optional field matches.")).toBeTruthy();
});

// Catches: a filter hides the one optional field that blocks Create, leaving it disabled with no visible reason.
it("optional_field_that_blocks_create_survives_the_filter_and_is_counted_in_the_summary", () => {
	const field = (id: string, name: string): WorkItemFieldSchema => ({
		id,
		name,
		kind: "text",
		required: false,
		editable: true,
		options: [],
	});
	const fields = [field("a", "Alpha"), field("b", "Bravo")];
	render(() => (
		<TaskCreateOptional fields={fields} error={(entry) => (entry.id === "b" ? "Invalid: Bravo" : undefined)}>
			{(entry) => <label>{entry.name}</label>}
		</TaskCreateOptional>
	));
	fireEvent.input(screen.getByLabelText("Filter optional fields"), { target: { value: "alpha" } });
	expect(screen.getByText("Bravo").closest("[hidden]")).toBeNull();
	fireEvent.input(screen.getByLabelText("Filter optional fields"), { target: { value: "zzz" } });
	expect(screen.getByText("Alpha").closest("[hidden]")).not.toBeNull();
	expect(screen.getByText("Bravo").closest("[hidden]")).toBeNull();
	expect(screen.getByText(/^Optional fields/)).toHaveTextContent("Optional fields (2) · 1 invalid");
});
