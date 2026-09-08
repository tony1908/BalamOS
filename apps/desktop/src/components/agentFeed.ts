import type {
  AgentEvent,
  AgentPlanStep,
  AgentSessionState,
  ApprovalDecision,
} from "../types";

export type AgentFeedItem =
  | { key: string; type: "message"; text: string }
  | { key: string; type: "user"; text: string }
  | {
      key: string;
      type: "approval";
      requestId: string;
      kind: "command" | "file_change";
      title: string;
      detail?: string | null;
      status: "pending" | "resolved";
      decision?: ApprovalDecision;
    }
  | {
      key: string;
      type: "plan";
      explanation?: string | null;
      steps: AgentPlanStep[];
    }
  | { key: string; type: "auth"; url: string; userCode?: string | null }
  | {
      key: string;
      type: "error";
      code: string;
      message: string;
      retryable: boolean;
    }
  | {
      key: string;
      type: "usage";
      inputTokens: number;
      cachedInputTokens: number;
      outputTokens: number;
    };

type FeedState = AgentSessionState | { state: AgentSessionState };

export function buildAgentFeed(
  events: readonly AgentEvent[],
  state: FeedState,
): AgentFeedItem[] {
  const currentState = typeof state === "string" ? state : state.state;
  const result: AgentFeedItem[] = [];
  const indexes = new Map<string, number>();
  const resolvedApprovals = new Map<string, ApprovalDecision>();
  let latestUsage: AgentFeedItem | undefined;
  let previousWasMessage = false;

  const append = (item: AgentFeedItem) => {
    result.push(item);
    indexes.set(item.key, result.length - 1);
  };
  const replace = (key: string, item: AgentFeedItem) => {
    const index = indexes.get(key);
    if (index === undefined) append(item);
    else result[index] = item;
  };

  for (const event of events) {
    const kind = event.kind;
    if (kind.type === "assistant_delta") {
      const key = `message:${event.cursor}`;
      if (previousWasMessage) {
        const prior = result[result.length - 1];
        if (prior?.type === "message") prior.text += kind.data.text;
      } else append({ key, type: "message", text: kind.data.text });
      previousWasMessage = true;
      continue;
    }
    previousWasMessage = false;
    switch (kind.type) {
      case "user_prompt":
        append({
          key: `user:${event.cursor}`,
          type: "user",
          text: kind.data.text,
        });
        break;
      case "approval_requested": {
        const key = `approval:${kind.data.request_id}`;
        const decision = resolvedApprovals.get(kind.data.request_id);
        replace(key, {
          key,
          type: "approval",
          requestId: kind.data.request_id,
          kind: kind.data.kind,
          title: kind.data.title,
          detail: kind.data.detail,
          status: decision ? "resolved" : "pending",
          ...(decision ? { decision } : {}),
        });
        break;
      }
      case "approval_resolved": {
        const key = `approval:${kind.data.request_id}`;
        resolvedApprovals.set(kind.data.request_id, kind.data.decision);
        const prior = result[indexes.get(key) ?? -1];
        if (prior?.type === "approval")
          replace(key, {
            ...prior,
            status: "resolved",
            decision: kind.data.decision,
          });
        break;
      }
      case "authentication_required":
        if (currentState === "needs_authentication")
          append({
            key: `auth:${event.cursor}`,
            type: "auth",
            url: kind.data.url,
            userCode: kind.data.user_code,
          });
        break;
      case "plan_updated":
        append({
          key: `plan:${event.cursor}`,
          type: "plan",
          explanation: kind.data.explanation,
          steps: kind.data.steps.map((step) => ({ ...step })),
        });
        break;
      case "error":
        append({
          key: `error:${event.cursor}`,
          type: "error",
          code: kind.data.code,
          message: kind.data.message,
          retryable: kind.data.retryable,
        });
        break;
      case "usage_updated":
        latestUsage = {
          key: "usage:latest",
          type: "usage",
          inputTokens: kind.data.input_tokens,
          cachedInputTokens: kind.data.cached_input_tokens,
          outputTokens: kind.data.output_tokens,
        };
        break;
      case "state_changed":
        break;
    }
  }
  if (latestUsage) result.push(latestUsage);
  return result;
}
