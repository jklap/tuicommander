/** Developer fixture: real core views + real Jira provider over recorded scrubbed REST replies, no live account. */
import { render } from "solid-js/web";
import "../../../../global.css";
import "../../../../styles.css";
import { detail, reference } from "../../detail/__tests__/detailFixture";
import { TasksDetail } from "../../detail/TasksDetail";
import { TasksCreate } from "../TasksCreate";
import { jiraFixtureProvider, recordedProjects, WIDE_LAN_TYPE } from "./jiraFixtureProvider";
import "./preview.css";

// ?project=LAN_WIDE|AITR&view=create|detail
const query = new URLSearchParams(location.search);
const project = query.get("project") === "AITR" ? "AITR" : "LAN_WIDE";
const { provider } = jiraFixtureProvider(project);
const root = document.getElementById("root");
if (!root) throw new Error("Missing fixture root");
if (query.get("view") === "detail") {
	const type = project === "AITR" ? Object.keys(recordedProjects.AITR.fields)[0] : WIDE_LAN_TYPE;
	const value = { ...detail(), fields: await provider.createFields(type) };
	render(
		() => (
			<div class="fixture-detail">
				<TasksDetail provider={{ ...provider, detail: async () => value }} reference={reference} />
			</div>
		),
		root,
	);
} else {
	render(() => <TasksCreate provider={provider} onCreated={() => {}} />, root);
	// The type picker is the only way into the field list; choose the same type the detail view uses.
	const type = project === "AITR" ? Object.keys(recordedProjects.AITR.fields)[0] : WIDE_LAN_TYPE;
	const picker = await new Promise<HTMLSelectElement>((resolve) => {
		const timer = setInterval(() => {
			const select = root.querySelector("select");
			if (select?.querySelector(`option[value="${type}"]`)) {
				clearInterval(timer);
				resolve(select);
			}
		}, 50);
	});
	picker.value = type;
	picker.dispatchEvent(new Event("change", { bubbles: true }));
}
