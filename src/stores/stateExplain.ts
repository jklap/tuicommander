import { createSignal } from "solid-js";

/** Which terminal's "Explain state" modal is open, if any. A plain signal
 *  rather than a full store: there is only ever at most one such modal open
 *  at a time, and every consumer (the Activity Dashboard row button, the
 *  terminal tab context menu) just needs to set/read one id. `StateExplainHost`
 *  is the single place that turns this id into the actual modal, mirroring
 *  `PtyOpenUrlHost`/`McpConfirmHost`'s "no UI of its own, mounted once
 *  globally" shape. */
const [openTermId, setOpenTermId] = createSignal<string | null>(null);

export const stateExplainStore = {
	openTermId,
	open(termId: string): void {
		setOpenTermId(termId);
	},
	close(): void {
		setOpenTermId(null);
	},
};
