import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RuntimeStatus, Workspace } from "../types";
import { useWorkspaces, type WorkspaceApi } from "./useWorkspaces";

const workspace = (
  id: string,
  state: Workspace["state"] = "stopped",
): Workspace => ({
  id,
  name: id,
  host_path: `/projects/${id}`,
  profile: "observe",
  harness: "codex",
  resources: {
    cpus: 1,
    memory_bytes: 1024,
    pids: 10,
    soft_disk_bytes: 100,
  },
  state,
});

const ready: RuntimeStatus = {
  available: true,
  mutations_ready: true,
  message: "ready",
};

function api(overrides: Partial<WorkspaceApi> = {}): WorkspaceApi {
  return {
    list: vi.fn().mockResolvedValue([]),
    create: vi.fn(),
    runtimeStatus: vi.fn().mockResolvedValue(ready),
    reconcile: vi.fn().mockResolvedValue(ready),
    start: vi.fn(),
    stop: vi.fn(),
    reset: vi.fn(),
    remove: vi.fn(),
    ...overrides,
  };
}

describe("useWorkspaces", () => {
  it("loads runtime and workspace metadata independently without reconciling", async () => {
    const controller = api({
      list: vi.fn().mockResolvedValue([workspace("a")]),
    });
    const { result } = renderHook(() => useWorkspaces(controller));

    await waitFor(() => expect(result.current.selectedId).toBe("a"));
    expect(controller.list).toHaveBeenCalledTimes(1);
    expect(controller.runtimeStatus).toHaveBeenCalledTimes(1);
    expect(controller.reconcile).not.toHaveBeenCalled();
  });

  it("fails closed while runtime readiness is unknown", async () => {
    const runtimeStatus = vi.fn(() => new Promise<RuntimeStatus>(() => {}));
    const start = vi.fn();
    const controller = api({ runtimeStatus, start });
    const { result } = renderHook(() => useWorkspaces(controller));

    expect(result.current.mutationsReady).toBe(false);
    await act(async () => result.current.start("a"));
    expect(start).not.toHaveBeenCalled();
  });

  it("retries through reconciliation and enables mutations only when ready", async () => {
    const unavailable: RuntimeStatus = {
      available: false,
      mutations_ready: false,
      message: "Docker stopped",
    };
    const controller = api({
      runtimeStatus: vi.fn().mockResolvedValue(unavailable),
    });
    const { result } = renderHook(() => useWorkspaces(controller));
    await waitFor(() => expect(result.current.runtime).toEqual(unavailable));
    expect(result.current.mutationsReady).toBe(false);

    await act(async () => result.current.retry());
    expect(controller.reconcile).toHaveBeenCalledTimes(1);
    expect(result.current.mutationsReady).toBe(true);
  });

  it("reloads workspace metadata after a successful retry", async () => {
    const row = workspace("recovered", "stopped");
    const list = vi.fn().mockResolvedValueOnce([]).mockResolvedValueOnce([row]);
    const controller = api({ list });
    const { result } = renderHook(() => useWorkspaces(controller));
    await waitFor(() => expect(result.current.runtime).toEqual(ready));

    await act(async () => result.current.retry());

    expect(list).toHaveBeenCalledTimes(2);
    expect(result.current.workspaces).toEqual([row]);
  });

  it("forwards the selected safety profile and harness when creating", async () => {
    const created = workspace("new");
    const controller = api({
      create: vi.fn().mockResolvedValue(created),
    });
    const { result } = renderHook(() => useWorkspaces(controller));
    await waitFor(() => expect(result.current.mutationsReady).toBe(true));

    await act(async () =>
      result.current.create("New", "/projects/new", "full_control", "opencode"),
    );
    expect(controller.create).toHaveBeenCalledWith(
      "New",
      "/projects/new",
      "full_control",
      "opencode",
      undefined,
      undefined,
    );
    expect(result.current.selectedId).toBe("new");
  });

  it.each([
    ["start", "start"],
    ["stop", "stop"],
    ["reset", "reset"],
  ] as const)("issues exactly one %s intent", async (action, method) => {
    const row = workspace("a", action === "stop" ? "stopped" : "running");
    const controller = api({
      list: vi.fn().mockResolvedValue([row]),
      [method]: vi.fn().mockResolvedValue(row),
    });
    const { result } = renderHook(() => useWorkspaces(controller));
    await waitFor(() => expect(result.current.mutationsReady).toBe(true));

    await act(async () => result.current[action]("a"));
    expect(controller[method]).toHaveBeenCalledTimes(1);
    expect(controller[method]).toHaveBeenCalledWith("a");
  });

  it("deduplicates same-tick lifecycle requests", async () => {
    let resolve!: (row: Workspace) => void;
    const start = vi.fn(
      () => new Promise<Workspace>((done) => (resolve = done)),
    );
    const controller = api({ start });
    const { result } = renderHook(() => useWorkspaces(controller));
    await waitFor(() => expect(result.current.mutationsReady).toBe(true));

    let first!: Promise<void>;
    await act(async () => {
      first = result.current.start("a");
      void result.current.start("a");
    });
    expect(start).toHaveBeenCalledTimes(1);
    await act(async () => resolve(workspace("a", "running")));
    await act(async () => first);
  });

  it("refreshes metadata after a failed mutation", async () => {
    const failure = new Error("start failed");
    const list = vi.fn().mockResolvedValue([workspace("a")]);
    const controller = api({
      list,
      start: vi.fn().mockRejectedValue(failure),
    });
    const { result } = renderHook(() => useWorkspaces(controller));
    await waitFor(() => expect(result.current.mutationsReady).toBe(true));

    await act(async () => {
      await expect(result.current.start("a")).rejects.toThrow("start failed");
    });
    expect(list).toHaveBeenCalledTimes(2);
    expect(result.current.error).toBe("start failed");
  });

  it("clears stale rows when refresh returns an empty list", async () => {
    const list = vi
      .fn()
      .mockResolvedValueOnce([workspace("a")])
      .mockResolvedValueOnce([]);
    const controller = api({
      list,
      reset: vi.fn().mockResolvedValue(workspace("a")),
    });
    const { result } = renderHook(() => useWorkspaces(controller));
    await waitFor(() => expect(result.current.selectedId).toBe("a"));
    await waitFor(() => expect(result.current.mutationsReady).toBe(true));

    await act(async () => result.current.reset("a"));
    expect(result.current.workspaces).toEqual([]);
    expect(result.current.selectedId).toBeNull();
  });
});
