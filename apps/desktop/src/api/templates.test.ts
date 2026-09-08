import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./botSettings", () => ({ botSettingsApi: { set: vi.fn() } }));
vi.mock("./routines", () => ({ routineApi: { create: vi.fn() } }));
vi.mock("./skills", () => ({ skillApi: { create: vi.fn() } }));

import { botSettingsApi } from "./botSettings";
import { routineApi } from "./routines";
import { skillApi } from "./skills";
import { applyTemplate, templateApi } from "./templates";

describe("templateApi", () => {
  beforeEach(() => invoke.mockReset().mockResolvedValue(undefined));

  it("uses the exact template commands and arguments", async () => {
    const settings = { display_name: "Bot", notifications: true };
    const routines = [
      {
        name: "Routine",
        instruction: "Do work",
        interval_minutes: 15,
        enabled: true,
      },
    ];
    const skills = [{ name: "Skill", instruction: "Help", enabled: true }];

    await templateApi.list();
    await templateApi.save("Template", settings, routines, skills);
    await templateApi.remove("template-id");

    expect(invoke.mock.calls).toEqual([
      ["list_templates"],
      ["save_template", { name: "Template", settings, routines, skills }],
      ["delete_template", { id: "template-id" }],
    ]);
  });

  it("applies template settings, routines, and skills", async () => {
    const template = {
      id: "t1",
      name: "Template",
      settings: {
        display_name: "Bot",
        label: "Primary",
        description: "A bot",
        notifications: false,
      },
      routines: [
        {
          name: "Routine",
          instruction: "Do work",
          interval_minutes: 15,
          enabled: true,
        },
      ],
      skills: [{ name: "Skill", instruction: "Help", enabled: false }],
    };

    await applyTemplate("ws-1", template);

    expect(botSettingsApi.set).toHaveBeenCalledWith("ws-1", {
      displayName: "Bot",
      label: "Primary",
      description: "A bot",
      notifications: false,
    });
    expect(routineApi.create).toHaveBeenCalledWith(
      "ws-1",
      "Routine",
      "Do work",
      15,
      true,
    );
    expect(skillApi.create).toHaveBeenCalledWith(
      "ws-1",
      "Skill",
      "Help",
      false,
    );
  });
});
