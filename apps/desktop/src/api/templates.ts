import { invoke } from "@tauri-apps/api/core";
import { botSettingsApi } from "./botSettings";
import { routineApi } from "./routines";
import { skillApi } from "./skills";
import type {
  Template,
  TemplateRoutine,
  TemplateSettings,
  TemplateSkill,
} from "../types";

export const templateApi = {
  list: () => invoke<Template[]>("list_templates"),
  save: (
    name: string,
    settings: TemplateSettings,
    routines: TemplateRoutine[],
    skills: TemplateSkill[],
  ) => invoke<Template>("save_template", { name, settings, routines, skills }),
  remove: (id: string) => invoke<string>("delete_template", { id }),
};

export async function applyTemplate(
  workspaceId: string,
  template: Template,
): Promise<void> {
  await botSettingsApi.set(workspaceId, {
    displayName: template.settings.display_name ?? undefined,
    label: template.settings.label ?? undefined,
    description: template.settings.description ?? undefined,
    notifications: template.settings.notifications,
  });
  for (const r of template.routines) {
    await routineApi.create(
      workspaceId,
      r.name,
      r.instruction,
      r.interval_minutes,
      r.enabled,
    );
  }
  for (const s of template.skills) {
    await skillApi.create(workspaceId, s.name, s.instruction, s.enabled);
  }
}
