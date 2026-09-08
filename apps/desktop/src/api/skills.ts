import { invoke } from "@tauri-apps/api/core";
import type { Skill } from "../types";

export const skillApi = {
  list: (workspaceId: string) =>
    invoke<Skill[]>("list_skills", { workspaceId }),
  create: (
    workspaceId: string,
    name: string,
    instruction: string,
    enabled: boolean,
  ) =>
    invoke<Skill>("create_skill", { workspaceId, name, instruction, enabled }),
  update: (id: string, name: string, instruction: string, enabled: boolean) =>
    invoke<Skill>("update_skill", { id, name, instruction, enabled }),
  remove: (id: string) => invoke<string>("delete_skill", { id }),
};
