import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { message, workspaceApi } from "./workspaces";

describe("workspaceApi", () => {
  beforeEach(() => invoke.mockReset().mockResolvedValue(undefined));

  it("uses the exact runtime commands", async () => {
    await workspaceApi.runtimeStatus();
    await workspaceApi.reconcile();
    expect(invoke).toHaveBeenNthCalledWith(1, "runtime_status");
    expect(invoke).toHaveBeenNthCalledWith(2, "reconcile_runtime");
  });

  it("uses explicit lifecycle intent commands", async () => {
    await workspaceApi.start("id");
    await workspaceApi.stop("id");
    await workspaceApi.reset("id");
    await workspaceApi.remove("id");
    expect(invoke.mock.calls).toEqual([
      ["start_workspace", { id: "id" }],
      ["stop_workspace", { id: "id" }],
      ["reset_workspace", { id: "id" }],
      ["delete_workspace", { id: "id" }],
    ]);
  });

  it("forwards profile and desktop-session identifiers exactly", async () => {
    await workspaceApi.create("Orbit", "/projects/orbit", "workspace", "codex");
    await workspaceApi.createDesktopSession("id");
    expect(invoke).toHaveBeenNthCalledWith(1, "create_workspace", {
      name: "Orbit",
      hostPath: "/projects/orbit",
      profile: "workspace",
      harness: "codex",
      model: null,
      reasoningEffort: null,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "create_desktop_session", {
      id: "id",
    });
  });

  it("forwards model and reasoning effort when set", async () => {
    await workspaceApi.create(
      "Orbit",
      "/projects/orbit",
      "workspace",
      "codex",
      "gpt-5-codex",
      "high",
    );
    expect(invoke).toHaveBeenNthCalledWith(1, "create_workspace", {
      name: "Orbit",
      hostPath: "/projects/orbit",
      profile: "workspace",
      harness: "codex",
      model: "gpt-5-codex",
      reasoningEffort: "high",
    });
  });

  it("lists apps and forwards workspace app selections", async () => {
    await workspaceApi.listApps();
    await workspaceApi.setWorkspaceApps("id", ["metamask"]);
    expect(invoke).toHaveBeenNthCalledWith(1, "list_apps");
    expect(invoke).toHaveBeenNthCalledWith(2, "set_workspace_apps", {
      workspaceId: "id",
      apps: ["metamask"],
    });
  });
});

describe("message", () => {
  it("returns a real daemon error message unchanged", () => {
    expect(
      message({
        code: "runtime_unavailable",
        message: "container runtime unavailable",
        retryable: true,
      }),
    ).toBe("container runtime unavailable");
  });

  it("explains the missing native backend for a browser-context invoke error", () => {
    const browserError = new TypeError(
      "Cannot read properties of undefined (reading 'invoke')",
    );
    expect(message(browserError)).toMatch(/native desktop app/);
  });
});
