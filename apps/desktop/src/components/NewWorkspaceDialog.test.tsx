import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { NewWorkspaceDialog } from "./NewWorkspaceDialog";
import { workspaceApi } from "../api/workspaces";
import type { Harness, PermissionProfile, Template } from "../types";

vi.mock("../api/workspaces", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api/workspaces")>();
  return {
    ...actual,
    workspaceApi: { ...actual.workspaceApi, listModels: vi.fn() },
  };
});

beforeEach(() => {
  vi.mocked(workspaceApi.listModels).mockReset().mockResolvedValue([]);
});

function setup(
  overrides: {
    mutationsReady?: boolean;
    onCreate?: (
      name: string,
      hostPath: string,
      profile: PermissionProfile,
      harness: Harness,
      template?: Template,
      avatarColor?: string,
      model?: string,
      reasoningEffort?: string,
    ) => Promise<void>;
    templates?: Template[];
    onClose?: () => void;
  } = {},
) {
  const onCreate = overrides.onCreate ?? vi.fn().mockResolvedValue(undefined);
  const onClose = overrides.onClose ?? vi.fn();
  render(
    <NewWorkspaceDialog
      open
      mutationsReady={overrides.mutationsReady ?? true}
      onCreate={onCreate}
      templates={overrides.templates}
      onClose={onClose}
    />,
  );
  return { onCreate, onClose };
}

describe("NewWorkspaceDialog", () => {
  it("submits name, host path, profile, and the selected harness", async () => {
    const user = userEvent.setup();
    const { onCreate, onClose } = setup();

    await user.type(screen.getByLabelText("Name"), "billing-api");
    await user.type(
      screen.getByLabelText("Project folder"),
      "/projects/billing",
    );
    await user.selectOptions(
      screen.getByLabelText("Safety profile"),
      "full_control",
    );
    await user.selectOptions(screen.getByLabelText("Harness"), "opencode");
    await user.click(screen.getByRole("button", { name: "Add agent" }));

    expect(onCreate).toHaveBeenCalledWith(
      "billing-api",
      "/projects/billing",
      "full_control",
      "opencode",
      undefined,
      "#529e85",
      "",
      "",
    );
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
  });

  it("submits with an empty host path when the project folder is blank", async () => {
    const user = userEvent.setup();
    const { onCreate } = setup();

    await user.type(screen.getByLabelText("Name"), "billing-api");
    expect(screen.getByRole("button", { name: "Add agent" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Add agent" }));

    expect(onCreate).toHaveBeenCalledWith(
      "billing-api",
      "",
      "workspace",
      "codex",
      undefined,
      "#529e85",
      "",
      "",
    );
  });

  it("defaults the harness to codex", async () => {
    const user = userEvent.setup();
    const { onCreate } = setup();

    await user.type(screen.getByLabelText("Name"), "billing-api");
    await user.type(
      screen.getByLabelText("Project folder"),
      "/projects/billing",
    );
    await user.click(screen.getByRole("button", { name: "Add agent" }));

    expect(onCreate).toHaveBeenCalledWith(
      "billing-api",
      "/projects/billing",
      "workspace",
      "codex",
      undefined,
      "#529e85",
      "",
      "",
    );
  });

  it("populates the Model dropdown with options fetched for the selected harness", async () => {
    vi.mocked(workspaceApi.listModels).mockResolvedValue([
      { id: "gpt-5-codex", label: "GPT-5 Codex" },
    ]);
    setup();

    await waitFor(() =>
      expect(workspaceApi.listModels).toHaveBeenCalledWith("codex"),
    );
    expect(screen.getByRole("option", { name: "GPT-5 Codex" })).toHaveValue(
      "gpt-5-codex",
    );
  });

  it("submits the free-text value when Custom… is selected for Model", async () => {
    const user = userEvent.setup();
    const { onCreate } = setup();
    await waitFor(() => expect(workspaceApi.listModels).toHaveBeenCalled());

    await user.selectOptions(screen.getByLabelText("Model"), "__custom__");
    await user.type(screen.getByLabelText("Custom model"), "my-custom-model");
    await user.type(screen.getByLabelText("Name"), "billing-api");
    await user.click(screen.getByRole("button", { name: "Add agent" }));

    expect(onCreate).toHaveBeenCalledWith(
      "billing-api",
      "",
      "workspace",
      "codex",
      undefined,
      "#529e85",
      "my-custom-model",
      "",
    );
  });

  it("offers Antigravity as a harness option", () => {
    setup();

    expect(screen.getByRole("option", { name: "Antigravity" })).toHaveValue(
      "antigravity",
    );
  });

  it("submits the selected template", async () => {
    const user = userEvent.setup();
    const template = {
      id: "t1",
      name: "My template",
      settings: { notifications: true },
      routines: [],
      skills: [],
    };
    const { onCreate } = setup({ templates: [template] });

    await user.selectOptions(
      screen.getByLabelText("Start from template"),
      "t1",
    );
    await user.type(screen.getByLabelText("Name"), "billing-api");
    await user.click(screen.getByRole("button", { name: "Add agent" }));

    expect(onCreate).toHaveBeenCalledWith(
      "billing-api",
      "",
      "workspace",
      "codex",
      template,
      "#529e85",
      "",
      "",
    );
  });

  it("submits a selected mascot color", async () => {
    const user = userEvent.setup();
    const { onCreate } = setup();

    await user.click(screen.getByLabelText("Mascot color #6b9bd2"));
    await user.type(screen.getByLabelText("Name"), "billing-api");
    await user.click(screen.getByRole("button", { name: "Add agent" }));

    expect(onCreate).toHaveBeenCalledWith(
      "billing-api",
      "",
      "workspace",
      "codex",
      undefined,
      "#6b9bd2",
      "",
      "",
    );
  });

  it("disables submission while daemon mutations are not ready", async () => {
    setup({ mutationsReady: false });
    expect(screen.getByRole("button", { name: "Add agent" })).toBeDisabled();
  });

  it("shows the required-fields note only while the name is empty", async () => {
    const user = userEvent.setup();
    setup({ mutationsReady: true });

    expect(screen.getByText("Enter a name to continue.")).toBeVisible();
    expect(screen.getByRole("button", { name: "Add agent" })).toBeDisabled();

    await user.type(screen.getByLabelText("Name"), "billing-api");

    expect(
      screen.queryByText("Enter a name to continue."),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add agent" })).toBeEnabled();
  });

  it("keeps user input and stays open after a rejected creation", async () => {
    const user = userEvent.setup();
    const onCreate = vi.fn().mockRejectedValue(new Error("path not found"));
    const { onClose } = setup({ onCreate });

    await user.type(screen.getByLabelText("Name"), "billing-api");
    await user.type(screen.getByLabelText("Project folder"), "/nope");
    await user.click(screen.getByRole("button", { name: "Add agent" }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Name")).toHaveValue("billing-api");
    expect(screen.getByLabelText("Project folder")).toHaveValue("/nope");
  });

  it("renders the message from a structured daemon error", async () => {
    const user = userEvent.setup();
    const onCreate = vi.fn().mockRejectedValue({
      code: "runtime_unavailable",
      message: "container runtime unavailable",
      retryable: true,
    });
    setup({ onCreate });

    await user.type(screen.getByLabelText("Name"), "billing-api");
    await user.click(screen.getByRole("button", { name: "Add agent" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "container runtime unavailable",
    );
    expect(screen.queryByText("[object Object]")).not.toBeInTheDocument();
  });
});
