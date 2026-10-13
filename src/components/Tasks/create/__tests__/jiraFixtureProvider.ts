import aitr from "../../../../../plugins/jira/fixtures/create-aitr.json";
import lan from "../../../../../plugins/jira/fixtures/create-lan.json";
import users from "../../../../../plugins/jira/fixtures/create-users.json";
// @ts-expect-error -- Built-in providers are distributed as plain JavaScript.
import { createJiraProvider } from "../../../../../plugins/jira/provider.js";
import type { WorkItemProvider } from "../../../../plugins/workItemProvider";

export const recordedProjects = { LAN: lan, AITR: aitr };

/** Real provider + recorded REST replies; every write is captured and rejected before networking. */
export function jiraFixtureProvider(project: keyof typeof recordedProjects) {
	const fixture = recordedProjects[project];
	const writes: unknown[] = [];
	const provider: WorkItemProvider = createJiraProvider({
		account: { id: "review", kind: "basic", host: "https://jira.example.invalid" },
		projectId: fixture.projectId,
		fetch: async (request: { url: string; method: string; body?: unknown }) => {
			const url = new URL(request.url);
			if (request.method !== "GET") {
				writes.push(request.body);
				return { status: 403, json: {} };
			}
			if (url.pathname.includes("/user/")) return { status: 200, json: users };
			const type = url.pathname.match(/\/issuetypes\/([^/]+)$/)?.[1];
			const pages: Record<string, unknown[]> = fixture.fields;
			return { status: 200, json: type ? pages[type]?.[0] : fixture.types };
		},
	});
	return { provider, writes };
}
