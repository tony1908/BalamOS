import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Routine } from "../types";
import { RoutinesPanel } from "./RoutinesPanel";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const routine: Routine = {
  id: "routine-1",
  workspace_id: "workspace-1",
  name: "Check pull requests",
  instruction: "Review open pull requests.",
  interval_minutes: 60,
  enabled: true,
};

describe("RoutinesPanel", () => {
  it("creates routines and toggles an existing routine", async () => {
    const user = userEvent.setup();
    const api = {
      list: vi.fn().mockResolvedValue([routine]),
      create: vi.fn().mockResolvedValue(routine),
      update: vi.fn().mockResolvedValue(routine),
      remove: vi.fn().mockResolvedValue(routine.id),
    };

    render(<RoutinesPanel workspaceId="workspace-1" api={api} />);

    expect(await screen.findByText("Check pull requests")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Create Routine" }));
    expect(screen.getByRole("textbox", { name: "Instruction" })).toBeVisible();

    await user.type(
      screen.getByRole("textbox", { name: "Name" }),
      "Daily sync",
    );
    await user.type(
      screen.getByRole("textbox", { name: "Instruction" }),
      "Sync the project.",
    );
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(api.create).toHaveBeenCalledWith(
      "workspace-1",
      "Daily sync",
      "Sync the project.",
      15,
      true,
    );
    await waitFor(() => expect(api.list).toHaveBeenCalledTimes(2));

    await user.click(
      screen.getByRole("checkbox", { name: /Check pull requests/ }),
    );
    expect(api.update).toHaveBeenCalledWith(
      routine.id,
      routine.name,
      routine.instruction,
      routine.interval_minutes,
      false,
    );
  });
});
