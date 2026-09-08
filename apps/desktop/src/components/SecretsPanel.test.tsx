import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SecretsPanel } from "./SecretsPanel";
import type { SecretsApi } from "../api/secrets";

const metadata = [{ id: "s1", name: "API key", env_name: "API_KEY" }];
function api(overrides: Partial<SecretsApi> = {}): SecretsApi {
  return {
    list: vi.fn().mockResolvedValue(metadata),
    create: vi.fn().mockResolvedValue(metadata[0]),
    replace: vi.fn().mockResolvedValue(undefined),
    remove: vi.fn().mockResolvedValue("s1"),
    assignments: vi.fn().mockResolvedValue([]),
    assign: vi.fn().mockResolvedValue({ secret_id: "s1", workspace_id: "w1" }),
    unassign: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

describe("SecretsPanel", () => {
  it("creates with a password and clears it after success", async () => {
    const mocked = api();
    render(<SecretsPanel open onClose={vi.fn()} api={mocked} />);
    await waitFor(() =>
      expect(screen.getByText("API key")).toBeInTheDocument(),
    );
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "New" },
    });
    fireEvent.change(screen.getByLabelText("Environment name"), {
      target: { value: "NEW" },
    });
    const input = screen.getByLabelText("Password value") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "secret" } });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => expect(input).toHaveValue(""));
  });

  it("shows replace and delete errors and assigns a workspace", async () => {
    const mocked = api({
      replace: vi.fn().mockRejectedValue(new Error("replace failed")),
      remove: vi.fn().mockRejectedValue(new Error("delete failed")),
    });
    render(
      <SecretsPanel open workspaceId="w1" onClose={vi.fn()} api={mocked} />,
    );
    await waitFor(() =>
      expect(screen.getByText("API key")).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("checkbox"));
    await waitFor(() => expect(mocked.assign).toHaveBeenCalledWith("s1", "w1"));
    fireEvent.click(screen.getByRole("button", { name: "Replace" }));
    fireEvent.change(screen.getByLabelText("Replacement password"), {
      target: { value: "x" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Replace value" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("replace failed"),
    );
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("delete failed"),
    );
  });

  it("checks and unchecks assignment after each mutation", async () => {
    const mocked = api();
    render(
      <SecretsPanel open workspaceId="w1" onClose={vi.fn()} api={mocked} />,
    );
    const checkbox = (await screen.findByRole("checkbox")) as HTMLInputElement;
    fireEvent.click(checkbox);
    await waitFor(() => expect(checkbox).toBeChecked());
    fireEvent.click(checkbox);
    await waitFor(() => expect(checkbox).not.toBeChecked());
    expect(mocked.assign).toHaveBeenCalledWith("s1", "w1");
    expect(mocked.unassign).toHaveBeenCalledWith("s1", "w1");
  });

  it("clears values on close and scope change and uses the dialog class", async () => {
    const mocked = api();
    const { rerender } = render(
      <SecretsPanel open workspaceId="w1" onClose={vi.fn()} api={mocked} />,
    );
    await screen.findByText("API key");
    expect(screen.getByRole("dialog")).toHaveClass("governance-dialog");
    fireEvent.change(screen.getByLabelText("Password value"), {
      target: { value: "secret" },
    });
    rerender(
      <SecretsPanel open workspaceId="w2" onClose={vi.fn()} api={mocked} />,
    );
    expect(screen.getByLabelText("Password value")).toHaveValue("");
    rerender(
      <SecretsPanel
        open={false}
        workspaceId="w2"
        onClose={vi.fn()}
        api={mocked}
      />,
    );
    rerender(
      <SecretsPanel open workspaceId="w2" onClose={vi.fn()} api={mocked} />,
    );
    expect(screen.getByLabelText("Password value")).toHaveValue("");
  });

  it("ignores a stale load after the panel closes", async () => {
    let resolve!: (value: typeof metadata) => void;
    const mocked = api({
      list: vi.fn().mockReturnValue(
        new Promise((r) => {
          resolve = r;
        }),
      ),
    });
    const { rerender } = render(
      <SecretsPanel open onClose={vi.fn()} api={mocked} />,
    );
    rerender(<SecretsPanel open={false} onClose={vi.fn()} api={mocked} />);
    resolve(metadata);
    await waitFor(() =>
      expect(screen.queryByText("API key")).not.toBeInTheDocument(),
    );
  });

  it("traps keyboard focus and returns focus on close", async () => {
    const trigger = document.createElement("button");
    document.body.appendChild(trigger);
    trigger.focus();
    const onClose = vi.fn();
    const { rerender } = render(<SecretsPanel open onClose={onClose} api={api()} />);
    await screen.findByText("API key");
    const close = screen.getByRole("button", { name: "Close secrets" });
    const last = screen.getByLabelText("Password value");
    last.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(close);
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(last);
    fireEvent.keyDown(document, { key: "Escape", stopPropagation: vi.fn() });
    expect(onClose).toHaveBeenCalled();
    rerender(<SecretsPanel open={false} onClose={onClose} api={api()} />);
    expect(document.activeElement).toBe(trigger);
    trigger.remove();
  });
});
