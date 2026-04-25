import { terminalsStore } from "../stores/terminals";
import { globalWorkspaceStore } from "../stores/globalWorkspace";
import { rateLimitStore } from "../stores/ratelimit";

export interface ActivityTerminalRow {
  id: string;
  name: string;
  shellState: string | null;
  awaitingInput: string | null;
  sessionId: string | null;
  agentType: string | null;
  agentIntent: string | null;
  currentTask: string | null;
  lastPrompt: string | null;
  activeSubTasks: number;
  cwd: string | null;
  lastDataAt: number | null;
  isActive: boolean;
  isRateLimited: boolean;
  isPromoted: boolean;
}

export interface ActivitySnapshot {
  terminals: ActivityTerminalRow[];
}

export function buildActivitySnapshot(): ActivitySnapshot {
  return {
    terminals: terminalsStore.getAttachedIds().map((id) => {
      const t = terminalsStore.get(id);
      return {
        id,
        name: t?.name ?? "",
        shellState: t?.shellState ?? null,
        awaitingInput: t?.awaitingInput ?? null,
        sessionId: t?.sessionId ?? null,
        agentType: t?.agentType ?? null,
        agentIntent: t?.agentIntent ?? null,
        currentTask: t?.agentType === "claude" ? null : (t?.currentTask ?? null),
        lastPrompt: t?.lastPrompt ?? null,
        activeSubTasks: t?.activeSubTasks ?? 0,
        cwd: t?.cwd ?? null,
        lastDataAt: terminalsStore.getLastDataAt(id),
        isActive: terminalsStore.state.activeId === id,
        isRateLimited: !!(t?.sessionId && rateLimitStore.isRateLimited(t.sessionId)),
        isPromoted: globalWorkspaceStore.isPromoted(id),
      };
    }),
  };
}
