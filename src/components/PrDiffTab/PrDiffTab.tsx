import { type Component, createMemo, createSignal, onMount, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { type DiffViewMode, uiStore } from "../../stores/ui";
import { cx } from "../../utils";
import { classifyDiffFile, parseLinguistGenerated } from "../../utils/diffFileScope";
import { DiffFileList } from "../shared/DiffFileList";
import { type DiffFileSection, parseDiffFiles } from "../ui/DiffViewer";
import s from "./PrDiffTab.module.css";

export interface PrDiffTabProps {
	prNumber: number;
	prTitle: string;
	diff: string;
	/** Local checkout, used to read `.gitattributes` for `linguist-generated` patterns. */
	repoPath?: string;
}

export const PrDiffTab: Component<PrDiffTabProps> = (props) => {
	const files = createMemo(() => parseDiffFiles(props.diff));
	const totalAdd = createMemo(() => files().reduce((sum, f) => sum + f.additions, 0));
	const totalDel = createMemo(() => files().reduce((sum, f) => sum + f.deletions, 0));

	const [generatedPatterns, setGeneratedPatterns] = createSignal<string[]>([]);
	// Best effort: the local checkout's .gitattributes stands in for the PR base's.
	onMount(() => {
		if (!props.repoPath) return;
		invoke<string>("read_file", { path: props.repoPath, file: ".gitattributes" })
			.then((text) => setGeneratedPatterns(parseLinguistGenerated(text)))
			.catch(() => {});
	});
	const collapsedByDefault = (file: DiffFileSection) => classifyDiffFile(file.path, generatedPatterns()) !== null;
	const collapsedCount = createMemo(() => files().filter(collapsedByDefault).length);

	const mode = (): DiffViewMode => uiStore.state.diffViewMode;

	const header = () => (
		<div class={s.header}>
			<span class={s.headerTitle}>
				#{props.prNumber} {props.prTitle}
			</span>
			<span class={s.headerStats}>
				{files().length} {t("prDiff.files", "files")} <span class={s.statAdd}>+{totalAdd()}</span>{" "}
				<span class={s.statDel}>-{totalDel()}</span>
			</span>
			<Show when={collapsedCount() > 0}>
				<span
					class={s.headerStats}
					title={t("prDiff.collapsedHint", "Lockfiles, generated and test files; click a file to expand")}
				>
					{collapsedCount()} {t("prDiff.collapsed", "collapsed")}
				</span>
			</Show>
			<div class={s.modeToggle}>
				<button
					class={cx(s.modeBtn, mode() === "split" && s.modeBtnActive)}
					onClick={() => uiStore.setDiffViewMode("split")}
					title={t("diffTab.splitView", "Side-by-side")}
				>
					<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
						<path d="M1 2h6v12H1V2zm8 0h6v12H9V2zM2 3v10h4V3H2zm8 0v10h4V3h-4z" />
					</svg>
				</button>
				<button
					class={cx(s.modeBtn, mode() === "unified" && s.modeBtnActive)}
					onClick={() => uiStore.setDiffViewMode("unified")}
					title={t("diffTab.unifiedView", "Inline")}
				>
					<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
						<path d="M1 2h14v12H1V2zm1 1v10h12V3H2z" />
					</svg>
				</button>
			</div>
		</div>
	);

	return (
		<Show
			when={files().length > 0}
			fallback={
				<div class={s.container}>
					{header()}
					<div class={s.emptyState}>{t("prDiff.empty", "No changes")}</div>
				</div>
			}
		>
			<DiffFileList
				files={files()}
				mode={mode()}
				header={header()}
				headerHeight={35}
				collapsedByDefault={collapsedByDefault}
			/>
		</Show>
	);
};
