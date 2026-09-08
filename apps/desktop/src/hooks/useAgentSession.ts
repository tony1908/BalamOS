import { useCallback, useEffect, useRef, useState } from "react";
import { agentApi } from "../api/agents";
import { message } from "../api/workspaces";
import type {
  AgentEvent,
  AgentEventBatch,
  AgentSession,
  ApprovalDecision,
  Workspace,
} from "../types";

export interface AgentApi {
  start: (workspaceId: string) => Promise<AgentSession>;
  prompt: (workspaceId: string, prompt: string) => Promise<AgentSession>;
  poll: (workspaceId: string, after: number) => Promise<AgentEventBatch>;
  resolve: (
    workspaceId: string,
    requestId: string,
    decision: ApprovalDecision,
  ) => Promise<AgentSession>;
  interrupt: (workspaceId: string) => Promise<AgentSession>;
  stop: (workspaceId: string) => Promise<string>;
}

export interface AgentController {
  session: AgentSession | null;
  events: AgentEvent[];
  error: string | null;
  loading: boolean;
  prompt: (text: string) => Promise<void>;
  resolve: (requestId: string, decision: ApprovalDecision) => Promise<void>;
  interrupt: () => Promise<void>;
  retry: () => Promise<void>;
}

export function useAgentSession(
  workspace: Workspace,
  api: AgentApi = agentApi,
): AgentController {
  const [session, setSession] = useState<AgentSession | null>(null);
  const [events, setEvents] = useState<AgentEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const generation = useRef(0);
  const cursor = useRef(0);
  const timer = useRef<number | null>(null);

  const start = useCallback(async () => {
    if (workspace.state !== "running") return;
    const request = generation.current;
    setLoading(true);
    setError(null);
    try {
      const next = await api.start(workspace.id);
      if (request === generation.current) setSession(next);
    } catch (cause) {
      if (request === generation.current) setError(message(cause));
    } finally {
      if (request === generation.current) setLoading(false);
    }
  }, [api, workspace.id, workspace.state]);

  useEffect(() => {
    generation.current += 1;
    cursor.current = 0;
    setSession(null);
    setEvents([]);
    setError(null);
    if (workspace.state === "running") void start();
    return () => {
      generation.current += 1;
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, [start, workspace.id, workspace.state]);

  useEffect(() => {
    if (!session || workspace.state !== "running") return;
    const request = generation.current;
    let cancelled = false;
    let hasMore = false;
    const poll = async () => {
      try {
        const batch = await api.poll(workspace.id, cursor.current);
        if (cancelled || request !== generation.current) return;
        hasMore = Boolean(batch.has_more);
        cursor.current = Math.max(cursor.current, batch.next_cursor);
        if (batch.events.length) {
          setEvents((current) => {
            const seen = new Set(current.map((event) => event.cursor));
            return [
              ...current,
              ...batch.events.filter((event) => !seen.has(event.cursor)),
            ].slice(-2_000);
          });
          let nextState: AgentSession["state"] | undefined;
          for (const event of [...batch.events].reverse()) {
            if (event.kind.type === "state_changed") {
              nextState = event.kind.data.state;
              break;
            }
          }
          if (nextState) {
            setSession((current) =>
              current
                ? {
                    ...current,
                    state: nextState,
                    next_cursor: batch.next_cursor,
                  }
                : current,
            );
          }
        }
        setError(null);
      } catch (cause) {
        if (!cancelled && request === generation.current) {
          setError(message(cause));
        }
      }
      if (!cancelled && request === generation.current) {
        if (hasMore) {
          timer.current = window.setTimeout(poll, 0);
        } else {
          const active =
            session.state === "running" ||
            session.state === "waiting_for_approval";
          timer.current = window.setTimeout(poll, active ? 400 : 2_000);
        }
      }
    };
    void poll();
    return () => {
      cancelled = true;
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, [
    api,
    session?.workspace_id,
    session?.state,
    workspace.id,
    workspace.state,
  ]);

  const run = async (operation: () => Promise<AgentSession>) => {
    setError(null);
    try {
      setSession(await operation());
    } catch (cause) {
      setError(message(cause));
      throw cause;
    }
  };

  return {
    session,
    events,
    error,
    loading,
    prompt: async (text) => run(() => api.prompt(workspace.id, text)),
    resolve: async (requestId, decision) =>
      run(() => api.resolve(workspace.id, requestId, decision)),
    interrupt: async () => run(() => api.interrupt(workspace.id)),
    retry: start,
  };
}
