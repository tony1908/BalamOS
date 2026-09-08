import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentSession } from "./useAgentSession";

const workspace = {
  id: "workspace-1",
  name: "Ubuntu",
  host_path: "/project",
  profile: "workspace" as const,
  harness: "codex" as const,
  resources: { cpus: 2, memory_bytes: 3, pids: 4, soft_disk_bytes: 5 },
  state: "running" as const,
};

describe("useAgentSession", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("starts a running workspace, polls by cursor, and appends events once", async () => {
    const api = {
      start: vi.fn().mockResolvedValue({
        workspace_id: workspace.id,
        thread_id: "thread-1",
        state: "idle",
        next_cursor: 0,
      }),
      prompt: vi.fn(),
      poll: vi.fn().mockResolvedValue({
        workspace_id: workspace.id,
        after: 0,
        next_cursor: 1,
        events: [
          {
            cursor: 1,
            kind: { type: "assistant_delta", data: { text: "hello" } },
          },
        ],
      }),
      resolve: vi.fn(),
      interrupt: vi.fn(),
      stop: vi.fn(),
    };
    const { result } = renderHook(() => useAgentSession(workspace, api));
    await waitFor(() => expect(api.start).toHaveBeenCalledWith(workspace.id));
    await waitFor(() => expect(result.current.events).toHaveLength(1));
    expect(result.current.events[0].cursor).toBe(1);
    expect(api.poll).toHaveBeenCalledWith(workspace.id, 0);
  });

  it("forwards prompt, approval, and interrupt actions", async () => {
    const session = {
      workspace_id: workspace.id,
      thread_id: "thread-1",
      state: "idle" as const,
      next_cursor: 0,
    };
    const api = {
      start: vi.fn().mockResolvedValue(session),
      prompt: vi.fn().mockResolvedValue({ ...session, state: "running" }),
      poll: vi.fn().mockResolvedValue({
        workspace_id: workspace.id,
        after: 0,
        next_cursor: 0,
        events: [],
      }),
      resolve: vi.fn().mockResolvedValue(session),
      interrupt: vi
        .fn()
        .mockResolvedValue({ ...session, state: "interrupted" }),
      stop: vi.fn(),
    };
    const { result } = renderHook(() => useAgentSession(workspace, api));
    await waitFor(() => expect(result.current.session).not.toBeNull());
    await act(() => result.current.prompt("work"));
    await act(() => result.current.resolve("approval", "accept"));
    await act(() => result.current.interrupt());
    expect(api.prompt).toHaveBeenCalledWith(workspace.id, "work");
    expect(api.resolve).toHaveBeenCalledWith(
      workspace.id,
      "approval",
      "accept",
    );
    expect(api.interrupt).toHaveBeenCalledWith(workspace.id);
  });
});
