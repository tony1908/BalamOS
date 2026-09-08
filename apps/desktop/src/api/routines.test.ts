import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { routineApi } from "./routines";

describe("routineApi", () => {
  beforeEach(() => invoke.mockReset().mockResolvedValue(undefined));

  it("uses the exact routine commands and arguments", async () => {
    await routineApi.list("workspace-id");
    await routineApi.create("workspace-id", "Name", "Do work", 15, true);
    await routineApi.update("routine-id", "Updated", "Do more", 30, false);
    await routineApi.remove("routine-id");

    expect(invoke.mock.calls).toEqual([
      ["list_routines", { workspaceId: "workspace-id" }],
      [
        "create_routine",
        {
          workspaceId: "workspace-id",
          name: "Name",
          instruction: "Do work",
          intervalMinutes: 15,
          enabled: true,
        },
      ],
      [
        "update_routine",
        {
          id: "routine-id",
          name: "Updated",
          instruction: "Do more",
          intervalMinutes: 30,
          enabled: false,
        },
      ],
      ["delete_routine", { id: "routine-id" }],
    ]);
  });
});
