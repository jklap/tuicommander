import { createSignal, For, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { notesStore } from "../../stores/notes";
import { rpc } from "../../transport";
import { sendCommand } from "../../utils/sendCommand";
import { formatRelativeTime } from "../../utils/time";
import { retryWrite } from "../utils/retryWrite";
import styles from "./IdeasOverlay.module.css";

interface IdeasOverlayProps {
	sessionId: string;
	repoPath: string | null;
	onDismiss: () => void;
}

export function IdeasOverlay(props: IdeasOverlayProps) {
	const [inputText, setInputText] = createSignal("");
	const [view, setView] = createSignal<"active" | "archived">("active");

	const notes = () =>
		view() === "active" ? notesStore.getActiveNotes(props.repoPath) : notesStore.getArchivedNotes(props.repoPath);
	const archivedCount = () => notesStore.archivedCount(props.repoPath);

	function handleBackdrop(e: MouseEvent) {
		if (e.target === e.currentTarget) props.onDismiss();
	}

	function handleSubmit() {
		const text = inputText().trim();
		if (!text) return;
		notesStore.addNote(text, props.repoPath, deriveDisplayName(props.repoPath));
		setInputText("");
	}

	function handleKeyDown(e: KeyboardEvent) {
		if (e.key === "Enter" && !e.shiftKey) {
			e.preventDefault();
			handleSubmit();
		}
		if (e.key === "Escape") {
			props.onDismiss();
		}
	}

	async function handleSend(note: { id: string; text: string }) {
		props.onDismiss();
		notesStore.archiveNote(note.id);
		try {
			// Route through the canonical sendCommand helper (split Enter for Ink
			// raw mode, bracketed-paste for multi-line, Windows-native Ctrl-U skip).
			await sendCommand((data) => retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data })), note.text);
		} catch (err) {
			appLogger.error("network", `Ideas send failed: ${err instanceof Error ? err.message : String(err)}`);
		}
	}

	return (
		<div class={styles.backdrop} onClick={handleBackdrop}>
			<div class={styles.sheet}>
				<div class={styles.header}>
					<span class={styles.title}>
						Ideas
						<Show when={notes().length > 0}>
							<span class={styles.badge}>{notes().length}</span>
						</Show>
					</span>
					<button class={styles.closeBtn} onClick={props.onDismiss}>
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
							<line x1="18" y1="6" x2="6" y2="18" />
							<line x1="6" y1="6" x2="18" y2="18" />
						</svg>
					</button>
				</div>

				<div class={styles.viewToggle}>
					<button
						class={styles.viewToggleBtn}
						classList={{ [styles.viewToggleActive]: view() === "active" }}
						onClick={() => setView("active")}
					>
						Active
					</button>
					<button
						class={styles.viewToggleBtn}
						classList={{ [styles.viewToggleActive]: view() === "archived" }}
						onClick={() => setView("archived")}
					>
						Archived
						<Show when={archivedCount() > 0}> ({archivedCount()})</Show>
					</button>
				</div>

				<div class={styles.list}>
					<Show when={notes().length === 0}>
						<div class={styles.empty}>
							{view() === "active" ? "No ideas yet. Add one below." : "No archived ideas."}
						</div>
					</Show>
					<For each={notes()}>
						{(note) => (
							<div class={styles.item}>
								<div class={styles.itemBody}>
									<span class={styles.itemText}>{note.text}</span>
									<div class={styles.itemMeta}>
										<span class={styles.itemDate}>
											{formatRelativeTime(note.createdAt, { showDateFallback: true })}
										</span>
										<Show when={note.repoDisplayName}>
											<span class={styles.itemProject}>{note.repoDisplayName}</span>
										</Show>
									</div>
								</div>
								<div class={styles.itemActions}>
									<Show
										when={view() === "active"}
										fallback={
											<button
												class={`${styles.actionBtn} ${styles.unarchiveBtn}`}
												onClick={() => notesStore.unarchiveNote(note.id)}
												title="Unarchive"
											>
												<svg
													width="14"
													height="14"
													viewBox="0 0 24 24"
													fill="none"
													stroke="currentColor"
													stroke-width="2"
												>
													<polyline points="9 14 4 9 9 4" />
													<path d="M20 20v-7a4 4 0 0 0-4-4H4" />
												</svg>
											</button>
										}
									>
										<button
											class={`${styles.actionBtn} ${styles.sendBtn}`}
											onClick={() => handleSend(note)}
											title="Send to terminal"
										>
											<svg
												width="14"
												height="14"
												viewBox="0 0 24 24"
												fill="none"
												stroke="currentColor"
												stroke-width="2"
											>
												<line x1="22" y1="2" x2="11" y2="13" />
												<polygon points="22 2 15 22 11 13 2 9 22 2" />
											</svg>
										</button>
										<button
											class={`${styles.actionBtn} ${styles.archiveBtn}`}
											onClick={() => notesStore.archiveNote(note.id)}
											title="Archive"
										>
											<svg
												width="14"
												height="14"
												viewBox="0 0 24 24"
												fill="none"
												stroke="currentColor"
												stroke-width="2"
											>
												<polyline points="21 8 21 21 3 21 3 8" />
												<rect x="1" y="3" width="22" height="5" />
												<line x1="10" y1="12" x2="14" y2="12" />
											</svg>
										</button>
									</Show>
									<button
										class={`${styles.actionBtn} ${styles.deleteBtn}`}
										onClick={() => notesStore.removeNote(note.id)}
										title="Delete"
									>
										<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
											<line x1="18" y1="6" x2="6" y2="18" />
											<line x1="6" y1="6" x2="18" y2="18" />
										</svg>
									</button>
								</div>
							</div>
						)}
					</For>
				</div>

				<Show when={view() === "active"}>
					<div class={styles.inputArea}>
						<textarea
							class={styles.input}
							rows={1}
							placeholder="Type an idea..."
							value={inputText()}
							onInput={(e) => setInputText(e.currentTarget.value)}
							onKeyDown={handleKeyDown}
						/>
						<button class={styles.submitBtn} onClick={handleSubmit} disabled={!inputText().trim()}>
							+
						</button>
					</div>
				</Show>
			</div>
		</div>
	);
}

function deriveDisplayName(repoPath: string | null): string | null {
	if (!repoPath) return null;
	return repoPath.split("/").filter(Boolean).pop() ?? repoPath;
}
