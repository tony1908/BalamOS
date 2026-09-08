import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { OrbitAppView } from "./App";
import type { WorkspaceController } from "./hooks/useWorkspaces";
import type { RuntimeStatus, Workspace } from "./types";

// Keep any agent/desktop IPC that mounted panes trigger inert during shell tests.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: () => new Promise(() => {}),
}));

const workspace = (
  id: string,
  name: string,
  hostPath: string,
  state: Workspace["state"] = "stopped",
): Workspace => ({
  id,
  name,
  host_path: hostPath,
  profile: "workspace",
  harness: "codex",
  resources: { cpus: 1, memory_bytes: 1024, pids: 10, soft_disk_bytes: 100 },
  state,
});

const ready: RuntimeStatus = {
  available: true,
  mutations_ready: true,
  message: "ready",
};

function controllerStub(
  overrides: Partial<WorkspaceController> = {},
): WorkspaceController {
  return {
    workspaces: [],
    selectedId: null,
    select: vi.fn(),
    create: vi.fn().mockResolvedValue(undefined),
    start: vi.fn().mockResolvedValue(undefined),
    stop: vi.fn().mockResolvedValue(undefined),
    reset: vi.fn().mockResolvedValue(undefined),
    remove: vi.fn().mockResolvedValue(undefined),
    applyUpdate: vi.fn(),
    loading: false,
    error: null,
    runtime: ready,
    runtimeLoading: false,
    retry: vi.fn().mockResolvedValue(undefined),
    pending: {},
    mutationsReady: true,
    ...overrides,
  };
}

const twoWorkspaces = () => [
  workspace("ws-1", "payments-api", "/projects/payments-api", "running"),
  workspace("ws-2", "orbit-website", "/projects/orbit-website", "stopped"),
];

describe("OrbitAppView", () => {
  beforeEach(() => {
    localStorage.setItem("balamos.onboarded", "true");
  });

  it("renders real workspace names and status labels, never mock data", async () => {
    render(
      <OrbitAppView
        controller={controllerStub({
          workspaces: twoWorkspaces(),
          selectedId: "ws-1",
        })}
      />,
    );

    const nav = await screen.findByRole("navigation", { name: "Workspaces" });
    expect(within(nav).getByText("payments-api")).toBeVisible();
    expect(within(nav).getByText("orbit-website")).toBeVisible();
    expect(within(nav).getByText("active")).toBeVisible();
    expect(within(nav).getByText("asleep")).toBeVisible();
    expect(screen.queryByText("feat/webhooks")).not.toBeInTheDocument();
    expect(
      screen.queryByText(/signature verification is passing/),
    ).not.toBeInTheDocument();
  });

  it("filters the list by workspace name and host path", async () => {
    const user = userEvent.setup();
    render(
      <OrbitAppView
        controller={controllerStub({
          workspaces: twoWorkspaces(),
          selectedId: "ws-1",
        })}
      />,
    );
    const list = screen.getByRole("navigation", { name: "Workspaces" });

    await user.type(screen.getByPlaceholderText("Search"), "orbit");
    expect(within(list).queryByText("payments-api")).not.toBeInTheDocument();
    expect(within(list).getByText("orbit-website")).toBeVisible();

    await user.clear(screen.getByPlaceholderText("Search"));
    await user.type(
      screen.getByPlaceholderText("Search"),
      "/projects/payments",
    );
    expect(within(list).getByText("payments-api")).toBeVisible();
    expect(within(list).queryByText("orbit-website")).not.toBeInTheDocument();
  });

  it("selects a workspace without invoking lifecycle actions", async () => {
    const user = userEvent.setup();
    const controller = controllerStub({
      workspaces: twoWorkspaces(),
      selectedId: "ws-1",
    });
    render(<OrbitAppView controller={controller} />);

    await user.click(
      within(screen.getByRole("navigation", { name: "Workspaces" })).getByText(
        "orbit-website",
      ),
    );

    expect(controller.select).toHaveBeenCalledWith("ws-2");
    expect(controller.start).not.toHaveBeenCalled();
    expect(controller.stop).not.toHaveBeenCalled();
  });

  it("exposes a persistent runtime status region", async () => {
    render(
      <OrbitAppView
        controller={controllerStub({
          workspaces: twoWorkspaces(),
          selectedId: "ws-1",
        })}
      />,
    );
    expect(screen.getByRole("status")).toBeInTheDocument();
  });

  it("offers Retry when the daemon is unavailable", async () => {
    const user = userEvent.setup();
    const retry = vi.fn().mockResolvedValue(undefined);
    render(
      <OrbitAppView
        controller={controllerStub({
          workspaces: [],
          selectedId: null,
          runtime: {
            available: false,
            mutations_ready: false,
            message: "Docker stopped",
          },
          mutationsReady: false,
          retry,
        })}
      />,
    );

    const overlay = screen.getByRole("alertdialog", {
      name: "Docker not running",
    });
    expect(screen.getByText(/We need Docker to work/)).toBeVisible();
    expect(
      within(overlay).getByRole("button", { name: "Retry" }),
    ).toBeVisible();
    expect(within(overlay).getByText("Docker stopped")).toBeVisible();
    await user.click(within(overlay).getByRole("button", { name: "Retry" }));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("shows a loader while connecting for the first time", () => {
    render(
      <OrbitAppView
        controller={controllerStub({ runtime: null, runtimeLoading: true })}
      />,
    );

    expect(screen.getByText("Connecting to BalamOS…")).toBeVisible();
    expect(screen.getByLabelText("Connecting")).toBeVisible();
  });

  it("does not show the Docker overlay when the daemon is available", () => {
    render(<OrbitAppView controller={controllerStub({ runtime: ready })} />);

    expect(
      screen.queryByText(/We need Docker to work/),
    ).not.toBeInTheDocument();
  });

  it("opens the creation dialog from the New agent action", async () => {
    const user = userEvent.setup();
    render(
      <OrbitAppView
        controller={controllerStub({
          workspaces: twoWorkspaces(),
          selectedId: "ws-1",
        })}
      />,
    );

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "New agent" }));
    expect(screen.getByRole("dialog")).toBeVisible();
  });

  it("opens global governance with no workspaces", async () => {
    const user = userEvent.setup();
    render(<OrbitAppView controller={controllerStub()} />);

    await user.click(screen.getByRole("button", { name: "Governance" }));
    expect(screen.getByRole("heading", { name: "Global governance" })).toBeVisible();
  });

  it("opens agent governance while the selected agent is stopped", async () => {
    const user = userEvent.setup();
    render(
      <OrbitAppView controller={controllerStub({
        workspaces: [workspace("ws-1", "stopped-agent", "/tmp/stopped")],
        selectedId: "ws-1",
      })} />,
    );

    await user.click(screen.getAllByRole("button", { name: "Governance" })[1]);
    expect(screen.getByRole("heading", { name: "Agent governance" })).toBeVisible();
  });

  it("uses semantic landmarks and hides decorative icons", () => {
    const { container } = render(
      <OrbitAppView
        controller={controllerStub({
          workspaces: twoWorkspaces(),
          selectedId: "ws-1",
        })}
      />,
    );

    expect(screen.getByRole("main")).toBeInTheDocument();
    expect(
      screen.getByRole("navigation", { name: "Workspaces" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("status")).toBeInTheDocument();

    const svgs = container.querySelectorAll("svg");
    expect(svgs.length).toBeGreaterThan(0);
    svgs.forEach((svg) => expect(svg).toHaveAttribute("aria-hidden", "true"));
  });

  it("opens conversation-first and reveals the OS column on demand", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <OrbitAppView
        controller={controllerStub({
          workspaces: twoWorkspaces(),
          selectedId: "ws-1",
        })}
      />,
    );
    const body = container.querySelector(".body")!;
    expect(body).toHaveClass("no-os");
    await user.click(screen.getByRole("button", { name: "Show live desktop" }));
    expect(body).toHaveClass("with-os");
    await user.click(screen.getByRole("button", { name: "Hide live desktop" }));
    expect(body).toHaveClass("no-os");
  });

  it("shows onboarding on first run and persists dismissal", async () => {
    const user = userEvent.setup();
    localStorage.removeItem("balamos.onboarded");
    const { rerender } = render(<OrbitAppView controller={controllerStub()} />);

    expect(
      screen.getByRole("dialog", { name: "Welcome to BalamOS" }),
    ).toBeVisible();
    for (let slide = 0; slide < 4; slide += 1) {
      await user.click(screen.getByRole("button", { name: "Next" }));
    }
    await user.click(
      screen.getByRole("button", { name: "Create your first bot" }),
    );
    expect(
      screen.queryByRole("dialog", { name: "Welcome to BalamOS" }),
    ).not.toBeInTheDocument();
    expect(localStorage.getItem("balamos.onboarded")).toBe("true");

    rerender(<OrbitAppView controller={controllerStub()} />);
    expect(
      screen.queryByRole("dialog", { name: "Welcome to BalamOS" }),
    ).not.toBeInTheDocument();
  });
});
