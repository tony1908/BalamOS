import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { GovernancePanel } from "./GovernancePanel";
import type { GovernanceView } from "../types";
import type { GovernanceApi } from "../api/governance";

const view: GovernanceView = {
  global: {
    enabled: true,
    require_approval: false,
    require_container: false,
    allow_scheduled: true,
    guidance: "",
  },
  local: {},
  effective: {
    enabled: true,
    require_approval: false,
    require_container: false,
    allow_scheduled: true,
    guidance: "",
    sources: {},
  },
  blocked_reasons: [],
  library: [{ id: "r1", title: "No prod", body: "Never touch prod." }],
};
const fake: GovernanceApi = {
  global: vi.fn().mockResolvedValue(view),
  saveGlobal: vi.fn().mockResolvedValue(view),
  workspace: vi.fn().mockResolvedValue(view),
  saveWorkspace: vi.fn().mockResolvedValue(view),
  listRules: vi.fn().mockResolvedValue(view.library),
  createRule: vi.fn().mockResolvedValue(view.library[0]),
  updateRule: vi.fn().mockResolvedValue(view.library[0]),
  deleteRule: vi.fn().mockResolvedValue(undefined),
};

describe("GovernancePanel", () => {
  it("renders the library rule title", async () => {
    render(
      <GovernancePanel open onClose={vi.fn()} workspaceId="w1" api={fake} />,
    );
    expect((await screen.findAllByText("No prod")).length).toBeGreaterThan(0);
  });
  it("creates a new library rule", async () => {
    const user = userEvent.setup();
    render(
      <GovernancePanel open onClose={vi.fn()} workspaceId="w1" api={fake} />,
    );
    await user.click(await screen.findByRole("button", { name: "+ Add rule" }));
    await user.type(
      await screen.findByLabelText("New rule title"),
      "No deploys",
    );
    await user.click(screen.getByRole("button", { name: "Add rule" }));
    expect(fake.createRule).toHaveBeenCalledWith("No deploys", "");
  });
  it("saves an applied library rule", async () => {
    const user = userEvent.setup();
    render(
      <GovernancePanel open onClose={vi.fn()} workspaceId="w1" api={fake} />,
    );
    await user.click(await screen.findByRole("checkbox", { name: "No prod" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(fake.saveWorkspace).toHaveBeenCalledWith(
      "w1",
      expect.objectContaining({ applied_rule_ids: ["r1"] }),
    );
  });
  it("saves a custom rule", async () => {
    const user = userEvent.setup();
    render(
      <GovernancePanel open onClose={vi.fn()} workspaceId="w1" api={fake} />,
    );
    await user.click(
      await screen.findByRole("button", { name: "+ Add custom rule" }),
    );
    await user.type(
      await screen.findByLabelText("Custom rule title"),
      "Be concise",
    );
    await user.click(screen.getByRole("button", { name: "Add custom rule" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(fake.saveWorkspace).toHaveBeenCalledWith(
      "w1",
      expect.objectContaining({
        custom_rules: expect.arrayContaining([
          expect.objectContaining({ title: "Be concise" }),
        ]),
      }),
    );
  });
});
