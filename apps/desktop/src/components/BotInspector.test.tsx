import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BotInspector } from "./BotInspector";
import { workspaceApi } from "../api/workspaces";
import type { BotSettings } from "../hooks/useBotSettings";
import type { DesktopSession, Workspace } from "../types";

vi.mock("../api/workspaces", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api/workspaces")>();
  return {
    ...actual,
    workspaceApi: {
      ...actual.workspaceApi,
      listModels: vi.fn(),
      setWorkspaceModel: vi.fn(),
      listApps: vi.fn(),
      setWorkspaceApps: vi.fn(),
    },
  };
});

beforeEach(() => {
  vi.mocked(workspaceApi.listModels).mockReset().mockResolvedValue([]);
  vi.mocked(workspaceApi.setWorkspaceModel).mockReset();
  vi.mocked(workspaceApi.listApps)
    .mockReset()
    .mockResolvedValue([
      {
        id: "metamask",
        label: "MetaMask",
        description: "Ethereum wallet (browser extension)",
      },
      {
        id: "obsidian",
        label: "Obsidian",
        description: "Markdown knowledge base (AppImage)",
      },
    ]);
  vi.mocked(workspaceApi.setWorkspaceApps).mockReset();
});

const workspace: Workspace = {
  id: "ws-1",
  name: "payments-api",
  host_path: "/projects/payments-api",
  profile: "workspace",
  harness: "opencode",
  resources: { cpus: 1, memory_bytes: 1024, pids: 10, soft_disk_bytes: 100 },
  state: "running",
};

const settings: BotSettings = { notifications: true };
const session: DesktopSession = {
  url: "https://desktop.test/ws-1",
  expires_at_unix_ms: Date.now() + 1_000_000,
};

function renderInspector(
  overrides: Partial<BotSettings> = {},
  onWorkspaceUpdated?: (ws: Workspace) => void,
) {
  const update = vi.fn();
  const createDesktopSession = vi.fn().mockResolvedValue(session);
  render(
    <BotInspector
      workspace={workspace}
      botSettings={{ settings: { ...settings, ...overrides }, update }}
      createDesktopSession={createDesktopSession}
      onWorkspaceUpdated={onWorkspaceUpdated}
    />,
  );
  return { update, createDesktopSession };
}

describe("BotInspector", () => {
  it("updates the display name when Name is edited", async () => {
    const user = userEvent.setup();
    const { update } = renderInspector();

    await user.clear(screen.getByLabelText("Name"));
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "Orbit" },
    });

    expect(update).toHaveBeenLastCalledWith({ displayName: "Orbit" });
  });

  it("updates notifications when toggled", async () => {
    const user = userEvent.setup();
    const { update } = renderInspector();

    await user.click(screen.getByLabelText("Notifications"));

    expect(update).toHaveBeenCalledWith({ notifications: false });
  });

  it("opens the share as template form from Settings", async () => {
    const user = userEvent.setup();
    renderInspector();

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("button", { name: "Share as template" }));

    expect(screen.getByLabelText("Template name")).toBeVisible();
    expect(screen.getByRole("button", { name: "Save template" })).toBeVisible();
  });

  it("saves the model and reasoning effort and propagates the updated workspace", async () => {
    vi.mocked(workspaceApi.listModels).mockResolvedValue([
      { id: "gpt-5-codex", label: "GPT-5 Codex" },
    ]);
    const updated = {
      ...workspace,
      model: "gpt-5-codex",
      reasoning_effort: "high",
    };
    vi.mocked(workspaceApi.setWorkspaceModel).mockResolvedValue(updated);
    const onWorkspaceUpdated = vi.fn();
    const user = userEvent.setup();
    renderInspector({}, onWorkspaceUpdated);

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await waitFor(() =>
      expect(workspaceApi.listModels).toHaveBeenCalledWith("opencode"),
    );
    await user.selectOptions(screen.getByLabelText("Model"), "gpt-5-codex");
    await user.selectOptions(screen.getByLabelText("Reasoning effort"), "high");
    await user.click(screen.getByRole("button", { name: "Save model" }));

    await waitFor(() =>
      expect(workspaceApi.setWorkspaceModel).toHaveBeenCalledWith(
        "ws-1",
        "gpt-5-codex",
        "high",
      ),
    );
    expect(onWorkspaceUpdated).toHaveBeenCalledWith(updated);
    expect(screen.getByText("Model saved ✓")).toBeVisible();
  });

  it("saves selected apps and propagates the updated workspace", async () => {
    const updated = { ...workspace, apps: ["metamask"] };
    vi.mocked(workspaceApi.setWorkspaceApps).mockResolvedValue(updated);
    const user = userEvent.setup();
    renderInspector();

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByLabelText("MetaMask"));
    await user.click(screen.getByRole("button", { name: "Save apps" }));

    await waitFor(() =>
      expect(workspaceApi.setWorkspaceApps).toHaveBeenCalledWith("ws-1", [
        "metamask",
      ]),
    );
  });

  it("switches between Settings and Screen views", async () => {
    const user = userEvent.setup();
    renderInspector();

    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByText("Runs on: opencode")).toBeVisible();
    expect(screen.getByLabelText("Name")).toBeVisible();
    expect(screen.getByText("Live desktop")).not.toBeVisible();

    await user.click(screen.getByRole("button", { name: "Screen" }));
    expect(screen.getByText("Live desktop")).toBeVisible();
    expect(
      screen.getByText("Read-only preview. Use Expand for a larger view."),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Terminal" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Files" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Ports" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Logs" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Available in Phase 4")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Name")).not.toBeVisible();
  });

  it("marks inactive inspector views hidden while Screen is active", () => {
    renderInspector();

    expect(screen.getByText(/Runs on:/).closest("[hidden]")).not.toBeNull();
  });

  it("shows the routines and skills panels in their views", async () => {
    const user = userEvent.setup();
    renderInspector();

    await user.click(screen.getByRole("button", { name: "Routines" }));

    expect(
      screen.getByText(
        "Routines are recurring tasks this bot runs on a schedule.",
      ),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Create Routine" }),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Skills" }));
    expect(
      screen.getByText("Skills are always-on capabilities this bot uses."),
    ).toBeVisible();
  });

  it("keeps the desktop mounted when switching views", async () => {
    const user = userEvent.setup();
    const { createDesktopSession } = renderInspector();
    await waitFor(() => expect(createDesktopSession).toHaveBeenCalledTimes(1));

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("button", { name: "Screen" }));

    expect(createDesktopSession).toHaveBeenCalledTimes(1);
  });

  it("expands and collapses the same desktop without creating another session", async () => {
    const user = userEvent.setup();
    const { createDesktopSession } = renderInspector({ displayName: "Orbit" });

    await waitFor(() => expect(createDesktopSession).toHaveBeenCalledTimes(1));
    expect(screen.getByText("Orbit's screen")).toBeVisible();
    expect(screen.getAllByTestId("desktop-stage")).toHaveLength(1);

    const stage = screen.getByTestId("screen-stage");
    expect(stage).not.toHaveClass("expanded");

    await user.click(screen.getByRole("button", { name: "Expand desktop" }));
    expect(screen.getByRole("button", { name: "Close desktop" })).toBeVisible();
    expect(stage).toHaveClass("expanded");
    expect(screen.getAllByTestId("desktop-stage")).toHaveLength(1);

    await user.click(screen.getByRole("button", { name: "Close desktop" }));
    expect(
      screen.getByRole("button", { name: "Expand desktop" }),
    ).toBeVisible();
    expect(stage).not.toHaveClass("expanded");
    expect(screen.getAllByTestId("desktop-stage")).toHaveLength(1);
    expect(createDesktopSession).toHaveBeenCalledTimes(1);
  });
});
