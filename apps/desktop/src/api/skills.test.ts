import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { skillApi } from "./skills";

describe("skillApi", () => {
  beforeEach(() => invoke.mockReset().mockResolvedValue(undefined));

  it("uses the exact skill commands and arguments", async () => {
    await skillApi.list("workspace-id");
    await skillApi.create("workspace-id", "Name", "Do work", true);
    await skillApi.update("skill-id", "Updated", "Do more", false);
    await skillApi.remove("skill-id");

    expect(invoke.mock.calls).toEqual([
      ["list_skills", { workspaceId: "workspace-id" }],
      [
        "create_skill",
        {
          workspaceId: "workspace-id",
          name: "Name",
          instruction: "Do work",
          enabled: true,
        },
      ],
      [
        "update_skill",
        {
          id: "skill-id",
          name: "Updated",
          instruction: "Do more",
          enabled: false,
        },
      ],
      ["delete_skill", { id: "skill-id" }],
    ]);
  });
});
