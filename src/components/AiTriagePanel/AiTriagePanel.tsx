import { Component, For, Show, createEffect, createMemo, createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { repositoriesStore } from "../../stores/repositories";
import { editorTabsStore } from "../../stores/editorTabs";
import { aiTriageStore, type FileClassification, type Relevance } from "../../stores/aiTriageStore";
import { cx } from "../../utils";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import p from "../shared/panel.module.css";
import s from "./AiTriagePanel.module.css";

function relevanceClass(r: Relevance): string {
  if (r === "high") return s.relevanceHigh;
  if (r === "medium") return s.relevanceMedium;
  return s.relevanceLow;
}

function statClass(r: Relevance): string {
  if (r === "high") return s.statHigh;
  if (r === "medium") return s.statMedium;
  return s.statLow;
}

function formatCategory(cat: string): string {
  return cat.replace(/-/g, " ");
}

export interface AiTriagePanelProps {
  visible: boolean;
  repoPath: string | null;
  onClose: () => void;
}

export const AiTriagePanel: Component<AiTriagePanelProps> = (props) => {
  createEffect(() => {
    if (!props.visible || !props.repoPath) return;
    const rev = repositoriesStore.getRevision(props.repoPath);
    void rev;
    void aiTriageStore.runTriage(props.repoPath);
  });

  const state = () => props.repoPath ? aiTriageStore.getState(props.repoPath) : { files: [], loading: false, llmUsed: false, llmModel: null, error: null };

  const highFiles = createMemo(() => state().files.filter((f) => f.relevance === "high"));
  const mediumFiles = createMemo(() => state().files.filter((f) => f.relevance === "medium"));
  const lowFiles = createMemo(() => state().files.filter((f) => f.relevance === "low"));

  const [expanded, setExpanded] = createSignal<Record<string, boolean>>({});
  const [diffs, setDiffs] = createSignal<Record<string, string>>({});
  const [lowGroupOpen, setLowGroupOpen] = createSignal(false);

  function isExpanded(path: string, relevance: Relevance): boolean {
    const e = expanded();
    if (path in e) return e[path];
    return relevance === "high";
  }

  async function toggleFile(file: FileClassification) {
    const wasOpen = isExpanded(file.path, file.relevance);
    setExpanded((prev) => ({ ...prev, [file.path]: !wasOpen }));

    if (!wasOpen && !diffs()[file.path] && props.repoPath) {
      try {
        const diff = await invoke<string>("get_file_diff", {
          path: props.repoPath,
          file: file.path,
        });
        setDiffs((prev) => ({ ...prev, [file.path]: diff }));
      } catch {
        setDiffs((prev) => ({ ...prev, [file.path]: "Failed to load diff" }));
      }
    }
  }

  function handleEdit(path: string) {
    if (props.repoPath) editorTabsStore.add(props.repoPath, path);
  }

  function handleRefresh() {
    if (props.repoPath) aiTriageStore.refreshTriage(props.repoPath);
  }

  function parseDiffLines(diff: string): Array<{ text: string; type: "add" | "del" | "hunk" | "context" }> {
    return diff.split("\n").map((line) => {
      if (line.startsWith("@@")) return { text: line, type: "hunk" as const };
      if (line.startsWith("+")) return { text: line, type: "add" as const };
      if (line.startsWith("-")) return { text: line, type: "del" as const };
      return { text: line, type: "context" as const };
    });
  }

  const diffLineClass = (type: string): string => {
    if (type === "add") return `${s.diffLine} ${s.diffAdd}`;
    if (type === "del") return `${s.diffLine} ${s.diffDel}`;
    if (type === "hunk") return `${s.diffLine} ${s.diffHunk}`;
    return `${s.diffLine} ${s.diffContext}`;
  };

  const FileRow: Component<{ file: FileClassification }> = (rowProps) => {
    const file = rowProps.file;
    const open = () => isExpanded(file.path, file.relevance);

    return (
      <div class={s.fileRow}>
        <div class={s.fileHeader} onClick={() => toggleFile(file)}>
          <span class={cx(s.chevron, open() && s.chevronOpen)}>&#9656;</span>
          <span class={cx(s.relevanceBadge, relevanceClass(file.relevance))}>
            {file.relevance}
          </span>
          <span class={s.categoryPill}>{formatCategory(file.category)}</span>
          <span class={s.filePath}>{file.path}</span>
          <span class={s.fileStats}>
            <Show when={file.additions > 0}>
              <span class={s.statsAdd}>+{file.additions}</span>
            </Show>
            <Show when={file.additions > 0 && file.deletions > 0}>{" "}</Show>
            <Show when={file.deletions > 0}>
              <span class={s.statsDel}>-{file.deletions}</span>
            </Show>
          </span>
          <div class={s.fileActions}>
            <button
              class={s.actionBtn}
              onClick={(e) => { e.stopPropagation(); handleEdit(file.path); }}
            >
              Edit
            </button>
          </div>
        </div>
        <Show when={file.summary}>
          <div class={s.fileSummary}>{file.summary}</div>
        </Show>
        <Show when={open() && diffs()[file.path]}>
          <div class={s.diffContainer}>
            <For each={parseDiffLines(diffs()[file.path])}>
              {(line) => <div class={diffLineClass(line.type)}>{line.text}</div>}
            </For>
          </div>
        </Show>
      </div>
    );
  };

  return (
    <div id="ai-triage-panel" class={cx(s.panel, !props.visible && s.hidden)}>
      <PanelResizeHandle panelId="ai-triage-panel" />
      <div class={p.header}>
        <div class={p.headerLeft}>
          <span class={p.title}>AI Triage</span>
          <Show when={highFiles().length > 0}>
            <span class={cx(s.statBadge, statClass("high"))}>{highFiles().length} high</span>
          </Show>
          <Show when={mediumFiles().length > 0}>
            <span class={cx(s.statBadge, statClass("medium"))}>{mediumFiles().length} med</span>
          </Show>
          <Show when={lowFiles().length > 0}>
            <span class={cx(s.statBadge, statClass("low"))}>{lowFiles().length} low</span>
          </Show>
        </div>
        <div class={p.headerRight}>
          <button class={s.refreshBtn} onClick={handleRefresh}>Refresh</button>
          <button class={p.close} onClick={props.onClose}>&times;</button>
        </div>
      </div>

      <div class={s.content}>
        <Show when={state().error}>
          <div class={s.error}>{state().error}</div>
        </Show>

        <Show when={state().loading}>
          <div class={s.loading}>
            <span class={s.spinner} />
            Classifying...
          </div>
        </Show>

        <Show when={!state().loading && state().files.length === 0 && !state().error}>
          <div class={s.empty}>No changes detected</div>
        </Show>

        <For each={highFiles()}>
          {(file) => <FileRow file={file} />}
        </For>

        <For each={mediumFiles()}>
          {(file) => <FileRow file={file} />}
        </For>

        <Show when={lowFiles().length > 0}>
          <div class={s.lowGroup}>
            <div class={s.lowGroupHeader} onClick={() => setLowGroupOpen(!lowGroupOpen())}>
              <span class={cx(s.chevron, lowGroupOpen() && s.chevronOpen)}>&#9656;</span>
              {lowFiles().length} low-relevance files
            </div>
            <Show when={lowGroupOpen()}>
              <div class={s.lowGroupContent}>
                <For each={lowFiles()}>
                  {(file) => <FileRow file={file} />}
                </For>
              </div>
            </Show>
          </div>
        </Show>

        <Show when={!state().llmUsed && state().files.length > 0}>
          <div class={s.banner}>
            Configure an AI provider in Settings to enable intelligent classification
          </div>
        </Show>
      </div>
    </div>
  );
};
