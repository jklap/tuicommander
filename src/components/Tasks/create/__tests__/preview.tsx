/** Developer fixture: real core form + A4 synthetic provider, no live account. */
import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import "../../../../global.css";
import "../../../../styles.css";
import { createInMemoryWorkItemProvider } from "../../../../plugins/testing/inMemoryWorkItemProvider";
import type { WorkItemProvider } from "../../../../plugins/workItemProvider";
import type { WorkItemFieldSchema, WorkItemRef } from "../../../../types/work-items.gen";
import { TasksDetail } from "../../detail/TasksDetail";
import { TasksCreate } from "../TasksCreate";
import "./preview.css";

const fake = createInMemoryWorkItemProvider();
const due: WorkItemFieldSchema = {
	id: "due",
	name: "Due date",
	kind: "date",
	required: true,
	editable: true,
	options: [],
};
const types: WorkItemFieldSchema = {
	id: "itemType",
	name: "Type",
	kind: "enum",
	required: true,
	editable: true,
	options: [
		{ id: "issue", name: "Issue" },
		{ id: "incident", name: "Incident" },
	],
};
// The A4 fake has one schema for every type; the fixture adds a required date to incidents and
// strips it before the fake validates the create, which rejects fields it does not declare.
const provider: WorkItemProvider = {
	...fake,
	createFields: async (itemType, signal) => {
		if (!itemType) return [types];
		const fields = await fake.createFields(itemType, signal);
		return itemType === "incident" ? [...fields, due] : fields;
	},
	act: (reference, action, signal) => {
		if (action.kind !== "create") return fake.act(reference, action, signal);
		const { due: _due, ...fields } = action.value.fields;
		return fake.act(reference, { kind: "create", value: { ...action.value, fields } }, signal);
	},
};

function Preview() {
	const [opened, setOpened] = createSignal<WorkItemRef | null>(null);
	return (
		<Show
			when={opened()}
			fallback={<TasksCreate provider={provider} onCreated={(detail) => setOpened(detail.item.reference)} />}
		>
			{(reference) => (
				<div class="fixture-detail">
					<TasksDetail provider={provider} reference={reference()} onClose={() => setOpened(null)} />
				</div>
			)}
		</Show>
	);
}
const root = document.getElementById("root");
if (!root) throw new Error("Missing fixture root");
render(() => <Preview />, root);
