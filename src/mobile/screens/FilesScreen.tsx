import { createSignal, For, onMount, Show } from "solid-js";
import { ContentRenderer } from "../../components/ui/ContentRenderer";
import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { rpc } from "../../transport";
import styles from "./FilesScreen.module.css";

interface FileEntry {
	name: string;
	path: string;
	is_dir: boolean;
	size: number;
}

const MAX_MOBILE_FILE_BYTES = 1_048_576;

interface FilesScreenProps {
	initialRepo?: { worktreePath: string | null; cwd: string | null };
	initialLink?: { candidate: string; line?: number };
	onExit?: () => void;
}

function normalizedPath(path: string): string {
	return path.replaceAll("\\", "/").replace(/\/+$/, "");
}

function withinRoot(path: string, root: string): boolean {
	const file = normalizedPath(path);
	const directory = normalizedPath(root);
	return file === directory || file.startsWith(`${directory}/`);
}

export function FilesScreen(props: FilesScreenProps) {
	const [repos, setRepos] = createSignal<string[]>([]);
	const [repo, setRepo] = createSignal<string | null>(null);
	const [dir, setDir] = createSignal("");
	const [entries, setEntries] = createSignal<FileEntry[]>([]);
	const [file, setFile] = createSignal<string | null>(null);
	const [content, setContent] = createSignal("");
	const [draft, setDraft] = createSignal("");
	const [editing, setEditing] = createSignal(false);
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	let requestId = 0;
	let editorEl: HTMLTextAreaElement | undefined;

	onMount(async () => {
		try {
			if (props.initialLink) {
				await openLinkedFile(props.initialLink);
				return;
			}
			const worktreePath = props.initialRepo?.worktreePath?.trim();
			if (worktreePath) {
				await openDirectory(worktreePath, "");
				return;
			}
			const cwd = props.initialRepo?.cwd?.trim();
			if (props.initialRepo && !cwd) {
				setError("Repository path is unavailable for this session.");
				return;
			}
			const config = await rpc<{ repos?: Record<string, unknown> }>("load_repositories");
			const registered = Object.keys(config.repos ?? {});
			if (cwd) {
				const matching = registered.filter((path) => withinRoot(cwd, path)).sort((a, b) => b.length - a.length);
				if (matching.length === 0) {
					setError("No registered repository contains this session directory.");
					return;
				}
				await openDirectory(matching[0], "");
			} else {
				setRepos(registered);
			}
		} catch (err) {
			setError(`Could not load repositories: ${String(err)}`);
		}
	});

	async function openLinkedFile(link: { candidate: string; line?: number }) {
		const cwd = props.initialRepo?.cwd?.trim() || props.initialRepo?.worktreePath?.trim();
		if (!cwd) {
			setError("Repository path is unavailable for this session.");
			return;
		}
		const resolved = await rpc<{ absolute_path: string; is_directory: boolean } | null>("resolve_terminal_path", {
			cwd,
			candidate: link.candidate,
		});
		if (!resolved || resolved.is_directory) {
			setError("Markdown file is unavailable.");
			return;
		}
		const config = await rpc<{ repos?: Record<string, unknown> }>("load_repositories");
		const roots = [props.initialRepo?.worktreePath, ...Object.keys(config.repos ?? {})].filter(
			(path): path is string => !!path,
		);
		const root = roots
			.filter((path) => withinRoot(resolved.absolute_path, path))
			.sort((a, b) => b.length - a.length)[0];
		if (!root) {
			setError("File is outside an allowed registered repository.");
			toastsStore.add(
				"Cannot open Markdown file",
				resolved.absolute_path,
				"error",
				false,
				undefined,
				undefined,
				undefined,
				undefined,
				false,
			);
			return;
		}
		const stat = await rpc<{ exists: boolean; is_dir: boolean; size: number }>("stat_path", {
			path: resolved.absolute_path,
		});
		if (!stat.exists || stat.is_dir) {
			setError("Markdown file is unavailable.");
			return;
		}
		const relative = normalizedPath(resolved.absolute_path).slice(normalizedPath(root).length + 1);
		setRepo(root);
		setDir(relative.split("/").slice(0, -1).join("/"));
		await openFile(
			{ name: relative.split("/").pop() ?? relative, path: relative, is_dir: false, size: stat.size },
			link.line,
		);
	}

	async function openDirectory(repoPath: string, subdir: string) {
		const currentRequest = ++requestId;
		setBusy(true);
		setError("");
		try {
			const result = await rpc<FileEntry[]>("list_directory", { repoPath, subdir });
			if (currentRequest !== requestId) return;
			setRepo(repoPath);
			setDir(subdir);
			setEntries(result);
		} catch (err) {
			if (currentRequest !== requestId) return;
			setError(`Could not open directory: ${String(err)}`);
		} finally {
			if (currentRequest === requestId) setBusy(false);
		}
	}

	async function openFile(entry: FileEntry, line?: number) {
		const currentRequest = ++requestId;
		setFile(entry.path);
		setContent("");
		setEditing(false);
		setError("");
		if (entry.size > MAX_MOBILE_FILE_BYTES) {
			setError("File too large to open on mobile (1 MB limit).");
			return;
		}
		setBusy(true);
		try {
			const result = await rpc<string>("fs_read_file", { repoPath: repo(), file: entry.path });
			if (currentRequest !== requestId) return;
			if (result.includes("\0")) {
				setError("This is a binary or non-text file.");
			} else {
				setContent(result);
				if (line) {
					setDraft(result);
					setEditing(true);
					queueMicrotask(() => {
						if (currentRequest !== requestId || !editorEl) return;
						const lines = result.split("\n");
						const target = Math.min(line - 1, lines.length - 1);
						const offset = lines.slice(0, target).reduce((sum, text) => sum + text.length + 1, 0);
						editorEl.focus();
						editorEl.setSelectionRange(offset, offset);
					});
				}
			}
		} catch (err) {
			if (currentRequest !== requestId) return;
			const message = String(err);
			setError(/valid UTF-8/i.test(message) ? "This is a binary or non-text file." : `Could not open file: ${message}`);
		} finally {
			if (currentRequest === requestId) setBusy(false);
		}
	}

	function goBack() {
		requestId++;
		setBusy(false);
		setError("");
		if (props.initialLink && props.onExit) {
			props.onExit();
			return;
		}
		if (file() !== null) {
			setFile(null);
			setEditing(false);
			return;
		}
		if (dir()) {
			void openDirectory(repo()!, dir().split("/").slice(0, -1).join("/"));
			return;
		}
		if (props.onExit) props.onExit();
		else setRepo(null);
	}

	async function save() {
		const repoPath = repo();
		const filePath = file();
		if (!repoPath || !filePath) return;
		setBusy(true);
		setError("");
		try {
			await rpc("write_file", { repoPath, file: filePath, content: draft() });
			setContent(draft());
			setEditing(false);
		} catch (err) {
			appLogger.warn("network", `Failed to save mobile file: ${String(err)}`);
			setError(`Could not save file: ${String(err)}`);
		} finally {
			setBusy(false);
		}
	}

	const repoName = (path: string) => path.split("/").filter(Boolean).pop() ?? path;

	return (
		<div class={styles.screen}>
			<header class={styles.header}>
				<Show when={repo() !== null || props.onExit}>
					<button class={styles.back} onClick={goBack} aria-label={props.onExit ? "Back to session" : "Back"}>
						‹ Back
					</button>
				</Show>
				<strong class={styles.title}>{file() || dir() || repoName(repo() ?? "") || "Files"}</strong>
			</header>
			<Show when={error()}>
				<p class={styles.error} role="alert">
					{error()}
				</p>
			</Show>
			<Show when={busy()}>
				<p class={styles.status}>Loading…</p>
			</Show>
			<Show when={repo() === null && !props.initialRepo}>
				<Show when={repos().length > 0} fallback={<p class={styles.status}>No repositories configured</p>}>
					<For each={repos()}>
						{(path) => (
							<button class={styles.row} onClick={() => void openDirectory(path, "")}>
								<span class={styles.name}>{repoName(path)}</span>
								<span class={styles.path}>{path}</span>
							</button>
						)}
					</For>
				</Show>
			</Show>
			<Show when={repo() !== null && file() === null}>
				<For each={entries()}>
					{(entry) => (
						<button
							class={styles.row}
							onClick={() => (entry.is_dir ? void openDirectory(repo()!, entry.path) : void openFile(entry))}
						>
							<span class={styles.name}>
								{entry.is_dir ? "▸ " : ""}
								{entry.name}
							</span>
						</button>
					)}
				</For>
				<Show when={entries().length === 0 && !busy() && !error()}>
					<p class={styles.status}>Empty directory</p>
				</Show>
			</Show>
			<Show when={file() !== null && (editing() || (!error() && !busy()))}>
				<div class={styles.actions}>
					<Show
						when={editing()}
						fallback={
							<button
								onClick={() => {
									setDraft(content());
									setEditing(true);
								}}
							>
								Edit
							</button>
						}
					>
						<button
							onClick={() => {
								setEditing(false);
								setError("");
							}}
						>
							Cancel
						</button>
						<button onClick={() => void save()}>Save</button>
					</Show>
				</div>
				<Show
					when={editing()}
					fallback={
						file()?.toLowerCase().endsWith(".md") ? (
							<div class={styles.markdownView}>
								<ContentRenderer content={content()} />
							</div>
						) : (
							<pre class={styles.viewer}>{content()}</pre>
						)
					}
				>
					<textarea
						ref={editorEl}
						class={styles.editor}
						aria-label="File content"
						value={draft()}
						onInput={(event) => setDraft(event.currentTarget.value)}
						spellcheck={false}
					/>
				</Show>
			</Show>
		</div>
	);
}
