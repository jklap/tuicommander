import { createMemo, Show } from "solid-js";
import { rateLimitStore } from "../../stores/ratelimit";
import { stateExplainStore } from "../../stores/stateExplain";
import { terminalsStore } from "../../stores/terminals";
import { StateExplainModal } from "./StateExplainModal";

/** No UI of its own outside the modal it conditionally renders — reads
 *  `stateExplainStore`'s open id and turns it into a `StateExplainModal`,
 *  same "mounted once globally" shape as `PtyOpenUrlHost`/`McpConfirmHost`.
 *  Every trigger (Activity Dashboard row button, terminal tab context menu)
 *  just calls `stateExplainStore.open(termId)`; this is the one place that
 *  reads `terminalsStore` for the frontend-badge comparison props.
 *
 *  The `term` lookup is a `createMemo`, not a plain read inside `<Show>`'s
 *  render-prop callback — that callback only re-runs when `openTermId()`
 *  itself changes, not when the looked-up terminal's own fields change.
 *  Code review caught that a plain read left the modal permanently blank if
 *  opened on a terminal whose PTY session was still spinning up (no
 *  `sessionId` yet): the callback saw `undefined` once and never re-checked.
 *  A memo re-runs on every reactive read inside it, so once `sessionId` is
 *  assigned, the nested `<Show>` below picks it up on the next tick. */
export function StateExplainHost() {
	const term = createMemo(() => {
		const id = stateExplainStore.openTermId();
		return id ? terminalsStore.get(id) : undefined;
	});

	return (
		<Show when={term()?.sessionId}>
			{(sessionIdAccessor) => {
				const sessionId = sessionIdAccessor();
				return (
					<StateExplainModal
						sessionId={sessionId}
						shellState={term()?.shellState ?? null}
						awaitingInput={term()?.awaitingInput ?? null}
						isRateLimited={rateLimitStore.isRateLimited(sessionId)}
						agentState={term()?.agentState ?? null}
						backgroundWork={term()?.backgroundWork ?? false}
						declaredBackgroundWork={term()?.declaredBackgroundWork ?? false}
						onClose={() => stateExplainStore.close()}
					/>
				);
			}}
		</Show>
	);
}

export default StateExplainHost;
