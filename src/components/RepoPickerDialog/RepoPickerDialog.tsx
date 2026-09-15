import { type Component, createEffect, For, Show } from "solid-js";
import { registerModal } from "../../stores/modalStack";
import d from "../shared/dialog.module.css";
import s from "./RepoPickerDialog.module.css";

export interface RepoPickerRepoOption {
	path: string;
	displayName: string;
}

export interface RepoPickerDialogProps {
	visible: boolean;
	/** The Finder-invoked path nothing could place automatically. */
	path: string;
	repos: RepoPickerRepoOption[];
	onChooseRepo: (repoPath: string) => void;
	onRegister: () => void;
	onUnattached: () => void;
	onClose: () => void;
}

/**
 * Asks which repo a Finder-invoked path should open under, when the
 * placement ladder (owning repo → active repo) found nothing. This is the
 * "ask the user" rung — see `resolvePlacementForCwd`
 * (`src/stores/terminalPlacement.ts`) for the two rungs above it.
 */
export const RepoPickerDialog: Component<RepoPickerDialogProps> = (props) => {
	createEffect(() => {
		if (!props.visible) return;

		// Escape-to-close is handled centrally (stores/modalStack): registering
		// routes Escape to props.onClose AND stops it reaching the terminal
		// underneath. No local keydown handler needed — the capture-phase
		// listener installed here already calls stopPropagation() before a
		// bubble-phase document listener could ever see the event.
		registerModal(props.onClose);
	});

	return (
		<Show when={props.visible}>
			<div class={d.overlay} onClick={props.onClose}>
				<div class={d.popover} onClick={(e) => e.stopPropagation()}>
					<div class={d.header}>
						<h4>Open terminal in which repo?</h4>
					</div>
					<div class={d.body}>
						<p class={s.pathLine}>{props.path}</p>
						<Show
							when={props.repos.length > 0}
							fallback={<p class={s.emptyState}>No repositories are registered yet.</p>}
						>
							<div class={s.repoList}>
								<For each={props.repos}>
									{(repo) => (
										<button type="button" class={s.repoButton} onClick={() => props.onChooseRepo(repo.path)}>
											<span>{repo.displayName}</span>
										</button>
									)}
								</For>
							</div>
						</Show>
						<div class={s.escapeHatches}>
							<button type="button" class={s.escapeHatchButton} onClick={props.onRegister}>
								Add this folder as a repository
							</button>
							<button type="button" class={s.escapeHatchButton} onClick={props.onUnattached}>
								Open unattached terminal
							</button>
						</div>
					</div>
					<div class={d.actions}>
						<button class={d.cancelBtn} onClick={props.onClose}>
							Cancel
						</button>
					</div>
				</div>
			</div>
		</Show>
	);
};

export default RepoPickerDialog;
