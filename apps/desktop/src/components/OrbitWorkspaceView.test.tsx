import { useState } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { OrbitWorkspaceView } from "./OrbitWorkspaceView";
import type { WorkspaceController } from "../hooks/useWorkspaces";
import type { DesktopSession, Workspace } from "../types";

// Agent/desktop IPC from mounted panes stays inert unless a test injects a stub.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: () => new Promise(() => {}),
}));

const ws = (
  state: Workspace["state"],
  harness: Workspace["harness"] = "codex",
): Workspace => ({
  id: "ws-1",
  name: "payments-api",
  host_path: "/projects/payments-api",
  profile: "workspace",
  harness,
  resources: { cpus: 1, memory_bytes: 1024, pids: 10, soft_disk_bytes: 100 },
  state,
});

function controllerStub(
  overrides: Partial<WorkspaceController> = {},
): WorkspaceController {
  return {
    workspaces: [],
    selectedId: "ws-1",
    select: vi.fn(),
    create: vi.fn().mockResolvedValue(undefined),
    start: vi.fn().mockResolvedValue(undefined),
    stop: vi.fn().mockResolvedValue(undefined),
    reset: vi.fn().mockResolvedValue(undefined),
    remove: vi.fn().mockResolvedValue(undefined),
    applyUpdate: vi.fn(),
    loading: false,
    error: null,
    runtime: { available: true, mutations_ready: true, message: "ready" },
    runtimeLoading: false,
    retry: vi.fn().mockResolvedValue(undefined),
    pending: {},
    mutationsReady: true,
    ...overrides,
  };
}

const desktopSession = (): DesktopSession => ({
  url: "https://desktop.test/ws-1",
  expires_at_unix_ms: Date.now() + 1_000_000,
});

function Harness({
  workspace,
  controller,
  createDesktopSession,
  startShown = true,
}: {
  workspace: Workspace;
  controller: WorkspaceController;
  createDesktopSession: (id: string) => Promise<DesktopSession>;
  startShown?: boolean;
}) {
  const [showOs, setShowOs] = useState(startShown);
  return (
    <OrbitWorkspaceView
      workspace={workspace}
      controller={controller}
      showOs={showOs}
      onToggleOs={() => setShowOs((value) => !value)}
      createDesktopSession={createDesktopSession}
      botSettings={{ settings: { notifications: true }, update: vi.fn() }}
    />
  );
}

function renderView(
  workspace: Workspace,
  controller = controllerStub(),
  createDesktopSession = vi.fn().mockResolvedValue(desktopSession()),
) {
  render(
    <Harness
      workspace={workspace}
      controller={controller}
      createDesktopSession={createDesktopSession}
    />,
  );
  return { controller, createDesktopSession };
}

describe("OrbitWorkspaceView lifecycle", () => {
  it("stops a running workspace by id", async () => {
    const user = userEvent.setup();
    const { controller } = renderView(ws("running"));
    await user.click(screen.getByRole("button", { name: "Bot actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Stop" }));
    expect(controller.stop).toHaveBeenCalledWith("ws-1");
  });

  it("starts a stopped workspace by id", async () => {
    const user = userEvent.setup();
    const { controller } = renderView(ws("stopped"));
    await user.click(screen.getByRole("button", { name: "Bot actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Start" }));
    expect(controller.start).toHaveBeenCalledWith("ws-1");
  });

  it("requires confirmation before resetting", async () => {
    const user = userEvent.setup();
    const { controller } = renderView(ws("running"));
    await user.click(screen.getByRole("button", { name: "Bot actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Reset" }));
    expect(controller.reset).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Confirm reset" }));
    expect(controller.reset).toHaveBeenCalledWith("ws-1");
  });

  it("requires confirmation before deleting", async () => {
    const user = userEvent.setup();
    const { controller } = renderView(ws("stopped"));
    await user.click(screen.getByRole("button", { name: "Bot actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(controller.remove).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Confirm delete" }));
    expect(controller.remove).toHaveBeenCalledWith("ws-1");
  });

  it("disables lifecycle controls until mutations are ready", async () => {
    const user = userEvent.setup();
    renderView(ws("running"), controllerStub({ mutationsReady: false }));
    await user.click(screen.getByRole("button", { name: "Bot actions" }));
    expect(screen.getByRole("menuitem", { name: "Stop" })).toBeDisabled();
  });
});

describe("OrbitWorkspaceView agent harness", () => {
  it("renders the agent conversation for the opencode harness", () => {
    renderView(ws("running", "opencode"));

    expect(
      screen.getByRole("log", { name: "Agent activity" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("region", { name: "Live desktop" }),
    ).toBeInTheDocument();
  });

  it("renders the agent conversation for the Codex harness", () => {
    renderView(ws("running", "codex"));

    expect(
      screen.getByRole("log", { name: "Agent activity" }),
    ).toBeInTheDocument();
  });
});

describe("OrbitWorkspaceView live desktop", () => {
  it("creates no desktop session until the desktop is first revealed", async () => {
    const user = userEvent.setup();
    const createDesktopSession = vi.fn().mockResolvedValue(desktopSession());
    render(
      <Harness
        workspace={ws("running")}
        controller={controllerStub()}
        createDesktopSession={createDesktopSession}
        startShown={false}
      />,
    );
    expect(createDesktopSession).not.toHaveBeenCalled();
    expect(screen.queryByTestId("desktop-stage")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Show live desktop" }));
    await waitFor(() => expect(createDesktopSession).toHaveBeenCalledTimes(1));

    await user.click(screen.getByRole("button", { name: "Hide live desktop" }));
    await user.click(screen.getByRole("button", { name: "Show live desktop" }));
    expect(createDesktopSession).toHaveBeenCalledTimes(1);
  });

  it("keeps one desktop session across hide and show", async () => {
    const user = userEvent.setup();
    const { createDesktopSession } = renderView(ws("running"));
    await waitFor(() => expect(createDesktopSession).toHaveBeenCalledTimes(1));
    expect(screen.getByTestId("desktop-stage")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Hide live desktop" }));
    await user.click(screen.getByRole("button", { name: "Show live desktop" }));

    expect(createDesktopSession).toHaveBeenCalledTimes(1);
    expect(screen.getAllByTestId("desktop-stage")).toHaveLength(1);
  });
});
