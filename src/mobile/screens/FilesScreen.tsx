import { createSignal, For, onMount, Show } from "solid-js";
import { ContentRenderer } from "../../components/ui/ContentRenderer";
import { appLogger } from "../../stores/appLogger";
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
	onExit?: () => void;
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

	onMount(async () => {
		try {
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
				const normalizedCwd = cwd.replaceAll("\\", "/").replace(/\/+$/, "");
				const matching = registered
					.filter((path) => {
						const root = path.replaceAll("\\", "/").replace(/\/+$/, "");
						return normalizedCwd === root || normalizedCwd.startsWith(`${root}/`);
					})
					.sort((a, b) => b.length - a.length);
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

	async function openFile(entry: FileEntry) {
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
