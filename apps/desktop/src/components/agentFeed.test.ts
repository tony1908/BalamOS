import { describe, expect, it } from "vitest";
import { buildAgentFeed } from "./agentFeed";
import type { AgentEvent, AgentEventKind, AgentPlanStep } from "../types";

const event = <K extends AgentEventKind["type"]>(
  cursor: number,
  type: K,
  data: Extract<AgentEventKind, { type: K }>["data"],
): AgentEvent => ({
  cursor,
  kind: { type, data } as Extract<AgentEventKind, { type: K }>,
});

describe("buildAgentFeed", () => {
  it("renders user prompt events before the response", () => {
    expect(
      buildAgentFeed(
        [
          event(1, "user_prompt", { text: "hi" }),
          event(2, "assistant_delta", { text: "hello" }),
        ],
        "running",
      ),
    ).toEqual([
      { key: "user:1", type: "user", text: "hi" },
      { key: "message:2", type: "message", text: "hello" },
    ]);
  });

  it("coalesces consecutive assistant deltas into one message", () => {
    expect(
      buildAgentFeed(
        [
          event(1, "assistant_delta", { text: "Hello " }),
          event(2, "assistant_delta", { text: "world" }),
        ],
        "running",
      ),
    ).toEqual([{ key: "message:1", type: "message", text: "Hello world" }]);
  });

  it("ignores tool events", () => {
    expect(
      buildAgentFeed(
        [
          event(1, "tool_started", { item_id: "b", title: "Second" }),
          event(2, "tool_started", { item_id: "a", title: "First" }),
          event(3, "tool_completed", {
            item_id: "a",
            success: false,
            detail: "failed",
          }),
          event(4, "tool_completed", {
            item_id: "b",
            success: true,
            detail: "done",
          }),
        ],
        "completed",
      ),
    ).toEqual([]);
  });

  it("marks resolved approvals so the UI can omit actions", () => {
    expect(
      buildAgentFeed(
        [
          event(1, "approval_requested", {
            request_id: "r1",
            kind: "command",
            title: "Run command",
          }),
          event(2, "approval_resolved", {
            request_id: "r1",
            decision: "accept",
          }),
        ],
        "idle",
      ),
    ).toEqual([
      {
        key: "approval:r1",
        type: "approval",
        requestId: "r1",
        kind: "command",
        title: "Run command",
        detail: undefined,
        status: "resolved",
        decision: "accept",
      },
    ]);
  });

  it("keeps only the latest usage item", () => {
    expect(
      buildAgentFeed(
        [
          event(0, "error", { code: "e", message: "oops", retryable: true }),
          event(1, "usage_updated", {
            input_tokens: 1,
            cached_input_tokens: 2,
            output_tokens: 3,
          }),
          event(2, "usage_updated", {
            input_tokens: 4,
            cached_input_tokens: 5,
            output_tokens: 6,
          }),
        ],
        "idle",
      ),
    ).toEqual([
      {
        key: "error:0",
        type: "error",
        code: "e",
        message: "oops",
        retryable: true,
      },
      {
        key: "usage:latest",
        type: "usage",
        inputTokens: 4,
        cachedInputTokens: 5,
        outputTokens: 6,
      },
    ]);
  });

  it("preserves approval resolution before request and duplicate requests", () => {
    const [approval] = buildAgentFeed(
      [
        event(1, "approval_resolved", { request_id: "r", decision: "accept" }),
        event(2, "approval_requested", {
          request_id: "r",
          kind: "command",
          title: "Run",
        }),
        event(3, "approval_requested", {
          request_id: "r",
          kind: "command",
          title: "Run again",
        }),
      ],
      "idle",
    );
    expect(approval).toMatchObject({
      status: "resolved",
      decision: "accept",
      title: "Run again",
    });
  });

  it("filters authentication unless current and preserves plan/error while cloning plans", () => {
    const steps: AgentPlanStep[] = [{ text: "One", state: "pending" }];
    const events = [
      event(1, "authentication_required", {
        url: "https://auth",
        user_code: "ABC",
      }),
      event(2, "plan_updated", { explanation: "Plan", steps }),
      event(3, "error", { code: "bad", message: "Nope", retryable: false }),
    ];
    expect(buildAgentFeed(events, "idle")).toHaveLength(2);
    const feed = buildAgentFeed(events, "needs_authentication");
    expect(feed).toHaveLength(3);
    const plan = feed.find((item) => item.type === "plan");
    if (plan?.type === "plan") plan.steps[0].text = "Changed";
    expect(steps[0].text).toBe("One");
  });
});
