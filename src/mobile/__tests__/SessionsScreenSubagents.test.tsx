// @vitest-environment jsdom

import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SessionsScreen } from "../screens/SessionsScreen";
import type { SessionInfo } from "../useSessions";
import { nestUnderParents } from "../utils/sessionTree";

const session = (id: string, name: string, extra: Partial<SessionInfo> = {}, agent = "claude"): SessionInfo => ({
	session_id: id,
	cwd: `/work/${id}`,
	worktree_path: null,
	worktree_branch: null,
	display_name: name,
	state: { awaiting_input: false, rate_limited: false, last_activity_ms: 0, agent_type: agent },
	...extra,
});

const renderScreen = (sessions: SessionInfo[]) =>
	render(() => (
		<SessionsScreen
			sessions={sessions}
			loading={false}
			refreshing={false}
			error={null}
			onRefresh={() => {}}
			onSelectSession={() => {}}
		/>
	));

beforeEach(() => localStorage.clear());
afterEach(cleanup);

describe("nestUnderParents", () => {
	// catches: child rendered as a top-level row apart from its parent
	it("places each child directly after the parent that spawned it, matched by tuic_session", () => {
		const rows = nestUnderParents([
			session("p", "Parent", { tuic_session: "tuic-p" }),
			session("other", "Other"),
			session("c1", "Child 1", { parent_session: "tuic-p" }),
			session("c2", "Child 2", { parent_session: "p" }),
		]);
		expect(rows.map((r) => [r.session.session_id, r.depth, r.last])).toEqual([
			["p", 0, true],
			["c1", 1, false],
			["c2", 1, true],
			["other", 0, true],
		]);
	});

	// catches: a child whose parent is absent or in a cycle vanishes from the list
	it("keeps orphans and parent cycles as visible top-level rows", () => {
		const rows = nestUnderParents([
			session("orphan", "Orphan", { parent_session: "gone" }),
			session("a", "A", { parent_session: "b" }),
			session("b", "B", { parent_session: "a" }),
			session("self", "Self", { parent_session: "self" }),
		]);
		expect(rows.map((r) => [r.session.session_id, r.depth])).toEqual([
			["orphan", 0],
			["a", 0],
			["b", 0],
			["self", 0],
		]);
	});
});

describe("sub-agent rows on the Sessions screen", () => {
	// catches: sub-agent has no marker, so the start of a group cannot be seen
	it("marks only children with the sub-agent icon, labelled with the parent name", () => {
		const view = renderScreen([
			session("p", "Orchestrator"),
			session("c", "Worker", { parent_session: "p" }),
			session("t", "Terminal", {}, "shell"),
		]);
		expect(view.getAllByRole("img", { name: "Spawned by Orchestrator" })).toHaveLength(1);
		expect(view.getAllByTestId("subagent-row")).toHaveLength(1);
	});

	// catches: the sort scatters a child away from its parent on the mobile list
	it("keeps a child under its parent although the sort would move it", () => {
		const view = renderScreen([
			session("p", "Orchestrator"),
			session("shell", "Plain shell", {}, "shell"),
			session("c", "Worker", { parent_session: "p" }, "shell"),
		]);
		const names = ["Orchestrator", "Worker", "Plain shell"];
		const order = names
			.map((n) => ({ n, node: view.getByText(n) }))
			.sort((a, b) => (a.node.compareDocumentPosition(b.node) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1))
			.map((e) => e.n);
		expect(order).toEqual(names);
	});
});
