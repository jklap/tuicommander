import { lazy, Show, Suspense } from "solid-js";
import { automationsUi } from "../../stores/automations";
import { repositoriesStore } from "../../stores/repositories";
import { getRepoConnection } from "../../transportRuntime";

const Dialog = lazy(() => import("./AutomationsDialog").then((module) => ({ default: module.AutomationsDialog })));
function OwnedDialog() {
	const connectionId = getRepoConnection(repositoriesStore.state.activeRepoPath);
	return <Dialog connectionId={connectionId} onClose={automationsUi.close} />;
}
export function AutomationsHost() {
	return (
		<Show when={automationsUi.visible()}>
			<Suspense>
				<OwnedDialog />
			</Suspense>
		</Show>
	);
}
