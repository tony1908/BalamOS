import { invoke } from "@tauri-apps/api/core";
import type { BotSettings } from "../hooks/useBotSettings";

type Wire = {
  workspace_id: string;
  display_name?: string | null;
  label?: string | null;
  description?: string | null;
  notifications: boolean;
  avatar_color?: string | null;
};

function fromWire(w: Wire): BotSettings {
  return {
    displayName: w.display_name ?? undefined,
    label: w.label ?? undefined,
    description: w.description ?? undefined,
    notifications: w.notifications,
    avatarColor: w.avatar_color ?? undefined,
  };
}

export const botSettingsApi = {
  list: async (): Promise<Record<string, BotSettings>> => {
    const rows = await invoke<Wire[]>("list_bot_settings");
    const out: Record<string, BotSettings> = {};
    for (const w of rows) out[w.workspace_id] = fromWire(w);
    return out;
  },
  set: async (workspaceId: string, s: BotSettings): Promise<BotSettings> => {
    const w = await invoke<Wire>("set_bot_settings", {
      workspaceId,
      displayName: s.displayName ?? null,
      label: s.label ?? null,
      description: s.description ?? null,
      notifications: s.notifications,
      avatarColor: s.avatarColor ?? null,
    });
    return fromWire(w);
  },
};
