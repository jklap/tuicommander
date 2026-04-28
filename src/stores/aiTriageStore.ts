import { createStore } from "solid-js/store";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// ---------------------------------------------------------------------------
// Types — mirror Rust diff_triage.rs
// ---------------------------------------------------------------------------

export type Relevance = "high" | "medium" | "low";
export type Category = "business-logic" | "api-surface" | "schema" | "config" | "test" | "boilerplate" | "style";
export type Risk = "breaking-change" | "behavioral-change" | "cosmetic";
export type ClassificationSource = "heuristic" | "llm";

export interface FileClassification {
  path: string;
  relevance: Relevance;
  category: Category;
  risk: Risk;
  summary: string;
  source: ClassificationSource;
  additions: number;
  deletions: number;
}

export interface TriageResult {
  summary: string | null;
  files: FileClassification[];
  llm_used: boolean;
  llm_model: string | null;
}

interface TriageProgress {
  repo_path: string;
  summary: string | null;
  files: FileClassification[];
  phase: string;
  done: boolean;
  llm_used: boolean;
  llm_model: string | null;
}

// ---------------------------------------------------------------------------
// Per-repo triage state
// ---------------------------------------------------------------------------

interface TriageState {
  summary: string | null;
  files: FileClassification[];
  loading: boolean;
  llmUsed: boolean;
  llmModel: string | null;
  error: string | null;
}

const DEFAULT_STATE: TriageState = {
  summary: null, files: [], loading: false, llmUsed: false, llmModel: null, error: null,
};

interface AiTriageStoreState {
  repos: Record<string, TriageState>;
}

const DEBOUNCE_MS = 2000;

function createAiTriageStore() {
  const [state, setState] = createStore<AiTriageStoreState>({ repos: {} });
  const pending = new Map<string, ReturnType<typeof setTimeout>>();
  const inflight = new Set<string>();

  // Listen for progressive triage events from Rust
  listen<TriageProgress>("triage-progress", (event) => {
    const p = event.payload;
    const repo = p.repo_path;
    const prev = state.repos[repo] ?? DEFAULT_STATE;

    // Merge: accumulate files from progressive events, replace on LLM-classified paths
    const existingByPath = new Map(prev.files.map((f) => [f.path, f]));
    for (const f of p.files) existingByPath.set(f.path, f);
    const merged = [...existingByPath.values()];
    merged.sort((a, b) => relevanceOrder(a.relevance) - relevanceOrder(b.relevance));

    setState("repos", repo, {
      summary: p.summary ?? prev.summary,
      files: merged,
      loading: !p.done,
      llmUsed: p.llm_used || prev.llmUsed,
      llmModel: p.llm_model ?? prev.llmModel,
      error: null,
    });
  });

  function relevanceOrder(r: Relevance): number {
    if (r === "high") return 0;
    if (r === "medium") return 1;
    return 2;
  }

  function getState(repoPath: string): TriageState {
    return state.repos[repoPath] ?? DEFAULT_STATE;
  }

  function runTriage(repoPath: string): void {
    if (pending.has(repoPath)) clearTimeout(pending.get(repoPath));
    pending.set(repoPath, setTimeout(() => {
      pending.delete(repoPath);
      void executeTriage(repoPath);
    }, DEBOUNCE_MS));
  }

  async function executeTriage(repoPath: string): Promise<void> {
    if (inflight.has(repoPath)) return;
    inflight.add(repoPath);
    const prev = getState(repoPath);
    setState("repos", repoPath, {
      ...prev,
      loading: true,
      error: null,
    });
    try {
      const result = await invoke<TriageResult>("run_diff_triage", { repoPath });
      // Final result — authoritative, replaces progressive state
      result.files.sort((a, b) => relevanceOrder(a.relevance) - relevanceOrder(b.relevance));
      setState("repos", repoPath, {
        summary: result.summary,
        files: result.files,
        loading: false,
        llmUsed: result.llm_used,
        llmModel: result.llm_model,
        error: null,
      });
    } catch (err) {
      setState("repos", repoPath, {
        ...getState(repoPath),
        loading: false,
        error: String(err),
      });
    } finally {
      inflight.delete(repoPath);
    }
  }

  function clear(repoPath: string): void {
    if (pending.has(repoPath)) {
      clearTimeout(pending.get(repoPath));
      pending.delete(repoPath);
    }
    setState("repos", repoPath, undefined!);
  }

  function refreshTriage(repoPath: string): void {
    if (pending.has(repoPath)) {
      clearTimeout(pending.get(repoPath));
      pending.delete(repoPath);
    }
    // Clear existing results for a fresh holistic analysis
    setState("repos", repoPath, { ...DEFAULT_STATE, loading: true });
    void executeTriage(repoPath);
  }

  return { state, getState, runTriage, refreshTriage, clear };
}

export const aiTriageStore = createAiTriageStore();
