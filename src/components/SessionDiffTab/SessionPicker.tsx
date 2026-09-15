import { type Component, createMemo, createSignal, Show } from "solid-js";
import type { SessionSummary } from "../../types/sessionDiff";
import { formatRelativeTime } from "../../utils/formatRelativeTime";
import { Dropdown, type DropdownItem } from "../ui/Dropdown";
import s from "./SessionDiffTab.module.css";

export interface SessionPickerProps {
	sessions: SessionSummary[];
	selectedId: string | null;
	/** The focused terminal's live agentSessionId, if any — shown with a dot. */
	liveId: string | null;
	loading: boolean;
	onSelect: (sessionId: string) => void;
	onRefresh: () => void;
}

function sessionLabel(s: SessionSummary): string {
	const title = s.title?.trim() || s.last_prompt?.trim().slice(0, 60) || s.session_id.slice(0, 8);
	const when = s.started_at ? formatRelativeTime(Date.now() - Date.parse(s.started_at)) : "";
	const counts = s.edit_count != null ? `${s.file_count ?? 0} files, ${s.edit_count} edits` : null;
	return [title, when, counts].filter(Boolean).join(" · ");
}

/** Header dropdown listing recent Claude Code sessions for the repo, so the
 *  reviewer can switch which session's edits are shown without opening a
 *  second tab. */
export const SessionPicker: Component<SessionPickerProps> = (props) => {
	const [open, setOpen] = createSignal(false);

	const items = createMemo<DropdownItem[]>(() =>
		props.sessions.map((sess) => ({
			id: sess.session_id,
			label: sess.session_id === props.liveId ? `● ${sessionLabel(sess)}` : sessionLabel(sess),
		})),
	);

	const selectedLabel = createMemo(() => {
		const found = props.sessions.find((sess) => sess.session_id === props.selectedId);
		if (props.loading) return "Loading sessions…";
		if (!found) return props.sessions.length === 0 ? "No Claude sessions found" : "Select a session";
		return sessionLabel(found);
	});

	return (
		<div class={s.sessionPicker}>
			<button
				type="button"
				class={s.sessionPickerTrigger}
				onClick={() => setOpen((v) => !v)}
				title="Choose which Claude Code session to review"
			>
				<span class={s.sessionPickerLabel}>{selectedLabel()}</span>
				<svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
					<path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
				</svg>
			</button>
			<Show when={props.sessions.length > 0}>
				<Dropdown
					items={items()}
					selected={props.selectedId ?? undefined}
					visible={open()}
					onSelect={(id) => {
						props.onSelect(id);
						setOpen(false);
					}}
					onClose={() => setOpen(false)}
				/>
			</Show>
			<button type="button" class={s.iconBtn} onClick={props.onRefresh} title="Refresh session list">
				<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
					<path d="M13.65 2.35A6 6 0 0 0 3 6h1.5A4.5 4.5 0 1 1 5.4 10.1L4 8.5v4h4l-1.65-1.65A6 6 0 1 0 13.65 2.35z" />
				</svg>
			</button>
		</div>
	);
};
