import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Skill } from "../types";
import { SkillsPanel } from "./SkillsPanel";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const skill: Skill = {
  id: "skill-1",
  workspace_id: "workspace-1",
  name: "Check pull requests",
  instruction: "Review open pull requests.",
  enabled: true,
};

describe("SkillsPanel", () => {
  it("creates skills and toggles an existing skill", async () => {
    const user = userEvent.setup();
    const api = {
      list: vi.fn().mockResolvedValue([skill]),
      create: vi.fn().mockResolvedValue(skill),
      update: vi.fn().mockResolvedValue(skill),
      remove: vi.fn().mockResolvedValue(skill.id),
    };

    render(<SkillsPanel workspaceId="workspace-1" api={api} />);
    expect(await screen.findByText(skill.name)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Create Skill" }));
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
      true,
    );
    await waitFor(() => expect(api.list).toHaveBeenCalledTimes(2));
    await user.click(screen.getByRole("checkbox", { name: skill.name }));
    expect(api.update).toHaveBeenCalledWith(
      skill.id,
      skill.name,
      skill.instruction,
      false,
    );
  });
});
