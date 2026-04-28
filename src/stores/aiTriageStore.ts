import { createStore } from "solid-js/store";
import { invoke } from "@tauri-apps/api/core";

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
  files: FileClassification[];
  llm_used: boolean;
  llm_model: string | null;
}

// ---------------------------------------------------------------------------
// Per-repo triage state
// ---------------------------------------------------------------------------

interface TriageState {
  files: FileClassification[];
  loading: boolean;
  llmUsed: boolean;
  llmModel: string | null;
  error: string | null;
}

interface AiTriageStoreState {
  repos: Record<string, TriageState>;
}

const DEBOUNCE_MS = 2000;

function createAiTriageStore() {
  const [state, setState] = createStore<AiTriageStoreState>({ repos: {} });
  const pending = new Map<string, ReturnType<typeof setTimeout>>();
  const inflight = new Set<string>();

  function getState(repoPath: string): TriageState {
    return state.repos[repoPath] ?? { files: [], loading: false, llmUsed: false, llmModel: null, error: null };
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
    setState("repos", repoPath, {
      files: getState(repoPath).files,
      loading: true,
      llmUsed: getState(repoPath).llmUsed,
      llmModel: getState(repoPath).llmModel,
      error: null,
    });
    try {
      const result = await invoke<TriageResult>("run_diff_triage", { repoPath });
      setState("repos", repoPath, {
        files: result.files,
        loading: false,
        llmUsed: result.llm_used,
        llmModel: result.llm_model,
        error: null,
      });
    } catch (err) {
      setState("repos", repoPath, {
        files: getState(repoPath).files,
        loading: false,
        llmUsed: false,
        llmModel: null,
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
    void executeTriage(repoPath);
  }

  return { state, getState, runTriage, refreshTriage, clear };
}

export const aiTriageStore = createAiTriageStore();
