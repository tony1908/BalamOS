import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { PluginsHub } from "./PluginsHub";
import { HEDERA_MAINNET_SKILL_NAME } from "../skills/hederaMainnet";
import type { SkillApiLike } from "../hooks/useSkills";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function fakeSkill(name: string) {
  return { id: "skill-1", workspace_id: "workspace-1", name, instruction: "x", enabled: true };
}

function api(overrides: Partial<SkillApiLike> = {}): SkillApiLike {
  return { list: vi.fn().mockResolvedValue([]), create: vi.fn().mockResolvedValue(fakeSkill("Circle Agent Wallets")), update: vi.fn(), remove: vi.fn(), ...overrides };
}

function renderHub(apiOverride: SkillApiLike, onOpen = vi.fn()) {
  render(<PluginsHub open workspaceId="workspace-1" onClose={vi.fn()} onOpenHedera={onOpen} api={apiOverride} />);
  return onOpen;
}

describe("PluginsHub", () => {
  it("shows a tile per plugin", async () => {
    renderHub(api());
    expect(await screen.findByRole("button", { name: "Circle Agent Wallets" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Hedera Mainnet Read-only" })).toBeInTheDocument();
  });

  it("installs a plugin from its detail view", async () => {
    const mocked = api();
    renderHub(mocked);
    await userEvent.click(await screen.findByRole("button", { name: "Circle Agent Wallets" }));
    await userEvent.click(await screen.findByRole("button", { name: "Add Circle Agent Wallets" }));
    await waitFor(() => expect(mocked.create).toHaveBeenCalledWith("workspace-1", "Circle Agent Wallets", expect.stringContaining("ARC-TESTNET"), true));
  });

  it("opens Hedera wallet & transfers from detail", async () => {
    const onOpen = vi.fn();
    renderHub(api(), onOpen);
    await userEvent.click(await screen.findByRole("button", { name: "Hedera Mainnet Read-only" }));
    await userEvent.click(screen.getByRole("button", { name: "Open wallet & transfers" }));
    expect(onOpen).toHaveBeenCalled();
  });

  it("already-installed plugin shows Installed and no install button", async () => {
    const mocked = api({ list: vi.fn().mockResolvedValue([fakeSkill(HEDERA_MAINNET_SKILL_NAME)]) });
    renderHub(mocked);
    await userEvent.click(await screen.findByRole("button", { name: "Hedera Mainnet Read-only" }));
    expect(screen.queryByRole("button", { name: "Add Hedera Mainnet skill" })).toBeNull();
    expect(screen.getByText("Installed")).toBeInTheDocument();
  });
});
