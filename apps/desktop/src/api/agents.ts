import { invoke } from "@tauri-apps/api/core";
import type { AgentEventBatch, AgentSession, ApprovalDecision } from "../types";

export const agentApi = {
  start: (workspaceId: string) =>
    invoke<AgentSession>("start_agent_session", { workspaceId }),
  prompt: (workspaceId: string, prompt: string) =>
    invoke<AgentSession>("send_agent_prompt", { workspaceId, prompt }),
  poll: (workspaceId: string, after: number) =>
    invoke<AgentEventBatch>("poll_agent_events", { workspaceId, after }),
  resolve: (
    workspaceId: string,
    requestId: string,
    decision: ApprovalDecision,
  ) =>
    invoke<AgentSession>("resolve_agent_approval", {
      workspaceId,
      requestId,
      decision,
    }),
  interrupt: (workspaceId: string) =>
    invoke<AgentSession>("interrupt_agent", { workspaceId }),
  stop: (workspaceId: string) =>
    invoke<string>("stop_agent_session", { workspaceId }),
};
