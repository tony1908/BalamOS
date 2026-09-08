import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { agentApi } from "./agents";

describe("agentApi", () => {
  beforeEach(() => invoke.mockReset().mockResolvedValue(undefined));

  it("uses exact commands and camelCase Tauri arguments", async () => {
    await agentApi.start("workspace-1");
    await agentApi.prompt("workspace-1", "hello");
    await agentApi.poll("workspace-1", 42);
    await agentApi.resolve("workspace-1", "approval-1", "accept_for_session");
    await agentApi.interrupt("workspace-1");
    await agentApi.stop("workspace-1");

    expect(invoke.mock.calls).toEqual([
      ["start_agent_session", { workspaceId: "workspace-1" }],
      ["send_agent_prompt", { workspaceId: "workspace-1", prompt: "hello" }],
      ["poll_agent_events", { workspaceId: "workspace-1", after: 42 }],
      [
        "resolve_agent_approval",
        {
          workspaceId: "workspace-1",
          requestId: "approval-1",
          decision: "accept_for_session",
        },
      ],
      ["interrupt_agent", { workspaceId: "workspace-1" }],
      ["stop_agent_session", { workspaceId: "workspace-1" }],
    ]);
  });
});
