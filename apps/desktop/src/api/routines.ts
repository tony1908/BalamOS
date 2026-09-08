import { invoke } from "@tauri-apps/api/core";
import type { Routine } from "../types";

export const routineApi = {
  list: (workspaceId: string) =>
    invoke<Routine[]>("list_routines", { workspaceId }),
  create: (
    workspaceId: string,
    name: string,
    instruction: string,
    intervalMinutes: number,
    enabled: boolean,
  ) =>
    invoke<Routine>("create_routine", {
      workspaceId,
      name,
      instruction,
      intervalMinutes,
      enabled,
    }),
  update: (
    id: string,
    name: string,
    instruction: string,
    intervalMinutes: number,
    enabled: boolean,
  ) =>
    invoke<Routine>("update_routine", {
      id,
      name,
      instruction,
      intervalMinutes,
      enabled,
    }),
  remove: (id: string) => invoke<string>("delete_routine", { id }),
};
