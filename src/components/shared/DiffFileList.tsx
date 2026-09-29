import { createVirtualizer } from "@tanstack/solid-virtual";
import { type Component, createMemo, createSignal, For, type JSX, Show } from "solid-js";
import type { DiffViewMode } from "../../stores/ui";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import { type DiffFileSection, DiffViewer } from "../ui/DiffViewer";
import s from "./diffFileList.module.css";

/** Reconstruct the raw diff string for a single file section. */
export function sectionToRawDiff(section: DiffFileSection): string {
	return section.lines.map((l) => l.content).join("\n");
}

/**
 * Stable per-row keys for a `DiffFileSection[]` list. Keying by `path` alone
 * collides when the same path appears twice (a partially-staged file shows
 * up once from the staged diff and once from the unstaged diff in
 * `BranchDiffScrollView`) — this disambiguates same-path duplicates by their
 * occurrence order, which is stable as long as relative ordering among
 * same-path entries doesn't change (it doesn't: staged is always listed
 * before unstaged).
 */
export function fileRowKeys(files: DiffFileSection[]): string[] {
	const counts = new Map<string, number>();
	return files.map((f) => {
		const path = f.path ?? "";
		const n = counts.get(path) ?? 0;
		counts.set(path, n + 1);
		return `${path}#${n}`;
	});
}

/** A single collapsible file diff. The chevron and header toggle collapse; the
 *  file path opens the file when `onOpen` is provided (working-tree view). */
const FileSection: Component<{
	file: DiffFileSection;
	mode: DiffViewMode;
	collapsed: boolean;
	onToggleCollapsed: () => void;
	onOpen?: () => void;
}> = (props) => {
	return (
		<div class={s.fileSection}>
			<div
				class={s.fileHeader}
				role="button"
				tabIndex={0}
				onClick={props.onToggleCollapsed}
				onKeyDown={onClickKeyDown(props.onToggleCollapsed)}
			>
				<svg
					class={cx(s.chevron, props.collapsed && s.chevronCollapsed)}
					width="12"
					height="12"
					viewBox="0 0 16 16"
					fill="currentColor"
				>
					<path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
				</svg>
				<span
					class={s.filePath}
					onClick={(e) => {
						if (!props.onOpen) return;
						e.stopPropagation();
						props.onOpen();
					}}
					style={props.onOpen ? { cursor: "pointer" } : undefined}
				>
					{props.file.path}
				</span>
				<span class={s.fileStats}>
					<Show when={props.file.additions > 0}>
						<span class={s.statAdd}>+{props.file.additions}</span>
					</Show>
					<Show when={props.file.deletions > 0}>
						<span class={s.statDel}>-{props.file.deletions}</span>
					</Show>
				</span>
			</div>
			<Show when={!props.collapsed}>
				<div class={s.fileDiff}>
					<DiffViewer diff={sectionToRawDiff(props.file)} mode={props.mode} />
				</div>
			</Show>
		</div>
	);
};

export interface DiffFileListProps {
	files: DiffFileSection[];
	mode: DiffViewMode;
	/** When provided, clicking a file path opens it (working-tree view). */
	onOpenFile?: (path: string) => void;
	/** Exposes the scroll container element (for Cmd+F search). */
	scrollRef?: (el: HTMLElement) => void;
	/** Optional content rendered above the list (sticky summary header). */
	header?: JSX.Element;
	/**
	 * Height in px of `header`, so per-file sticky headers stick right below
	 * it instead of at (or overlapping) the true top. Omit (or 0) when there
	 * is no header.
	 */
	headerHeight?: number;
	/**
	 * Collapse state keyed by `fileRowKeys()`, owned by the parent. When
	 * omitted, `DiffFileList` keeps its own internal set (still keyed by row
	 * key, not by list position) so simple callers don't need to wire this up.
	 */
	collapsedKeys?: ReadonlySet<string>;
	onToggleCollapsed?: (key: string) => void;
}

/**
 * Virtualized list of per-file diffs. Only sections inside the scroll viewport
 * (plus overscan) are mounted — a 100-file diff parses + renders ~5 DiffViewers
 * instead of 100. Items use `top` (not `transform`) positioning so the per-file
 * `position: sticky` headers keep working.
 *
 * DEFERRED (2026-06-07) — Cmd+F via DomSearchEngine only matches *mounted*
 * sections in this virtualized list (off-screen files aren't in the DOM). A
 * complete fix needs data-level search over parseDiffFiles output + scroll-to;
 * that's a separate change. Surfaced here rather than degrading silently.
 */
export const DiffFileList: Component<DiffFileListProps> = (props) => {
	let scrollEl: HTMLDivElement | undefined;

	const keys = createMemo(() => fileRowKeys(props.files));

	// Fallback collapse state, used only when the parent doesn't own it —
	// keyed the same way as the parent-owned path, never by list position.
	const [ownCollapsed, setOwnCollapsed] = createSignal<Set<string>>(new Set());
	const collapsedKeys = () => props.collapsedKeys ?? ownCollapsed();
	const toggleCollapsed = (key: string) => {
		if (props.onToggleCollapsed) {
			props.onToggleCollapsed(key);
			return;
		}
		setOwnCollapsed((prev) => {
			const next = new Set(prev);
			if (next.has(key)) next.delete(key);
			else next.add(key);
			return next;
		});
	};

	const virtualizer = createVirtualizer({
		get count() {
			return props.files.length;
		},
		getScrollElement: () => scrollEl ?? null,
		estimateSize: () => 320,
		overscan: 3,
		getItemKey: (i) => keys()[i] ?? i,
	});

	return (
		<div
			class={s.container}
			style={{ "--diff-header-height": `${props.headerHeight ?? 0}px` }}
			ref={(el) => {
				scrollEl = el;
				props.scrollRef?.(el);
			}}
		>
			{props.header}
			<div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative", width: "100%" }}>
				<For each={virtualizer.getVirtualItems()}>
					{(vi) => {
						const key = () => keys()[vi.index];
						return (
							<div
								data-index={vi.index}
								ref={(el) => virtualizer.measureElement(el)}
								style={{ position: "absolute", top: `${vi.start}px`, left: "0", width: "100%" }}
							>
								<Show when={key()} keyed>
									{() => (
										<FileSection
											file={props.files[vi.index]}
											mode={props.mode}
											collapsed={collapsedKeys().has(key())}
											onToggleCollapsed={() => toggleCollapsed(key())}
											onOpen={props.onOpenFile ? () => props.onOpenFile?.(props.files[vi.index].path) : undefined}
										/>
									)}
								</Show>
							</div>
						);
					}}
				</For>
			</div>
		</div>
	);
};
