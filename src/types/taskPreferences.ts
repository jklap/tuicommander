import type { WorkItemQuery } from "./work-items.gen";

/** Host UI preferences; keys contain the repository, provider and account identity. */
export interface TaskViewPreference {
	source?: string;
	view?: "list" | "board" | "roadmap";
	query?: WorkItemQuery;
	lastValidSort?: WorkItemQuery["sort"];
	linkedOnly?: boolean;
}
