import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { botSettingsApi } from "./botSettings";

describe("botSettingsApi", () => {
  beforeEach(() => invoke.mockReset());

  it("lists and maps daemon settings", async () => {
    invoke.mockResolvedValue([
      {
        workspace_id: "ws-a",
        display_name: "Alpha",
        label: null,
        description: "A bot",
        notifications: true,
      },
    ]);

    await expect(botSettingsApi.list()).resolves.toEqual({
      "ws-a": {
        displayName: "Alpha",
        description: "A bot",
        notifications: true,
      },
    });
    expect(invoke).toHaveBeenCalledWith("list_bot_settings");
  });

  it("sets daemon settings with wire arguments", async () => {
    invoke.mockResolvedValue({
      workspace_id: "ws-a",
      display_name: "Alpha",
      notifications: false,
    });

    await botSettingsApi.set("ws-a", {
      displayName: "Alpha",
      notifications: false,
    });

    expect(invoke).toHaveBeenCalledWith("set_bot_settings", {
      workspaceId: "ws-a",
      displayName: "Alpha",
      label: null,
      description: null,
      notifications: false,
      avatarColor: null,
    });
  });
});
