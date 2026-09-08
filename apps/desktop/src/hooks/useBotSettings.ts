import { useCallback, useEffect, useState } from "react";
import { botSettingsApi } from "../api/botSettings";

export type BotSettings = {
  displayName?: string;
  label?: string;
  description?: string;
  notifications: boolean;
  avatarColor?: string;
};

const defaults: BotSettings = { notifications: true };

export function useBotSettings() {
  const [map, setMap] = useState<Record<string, BotSettings>>({});
  const [loading, setLoading] = useState(true);

  const reload = useCallback(() => {
    let active = true;
    botSettingsApi
      .list()
      .then((all) => {
        if (active) setMap(all);
      })
      .catch(() => {
        // Daemon unreachable; defaults apply.
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const get = useCallback(
    (workspaceId: string): BotSettings => map[workspaceId] ?? defaults,
    [map],
  );

  const update = useCallback(
    (workspaceId: string, patch: Partial<BotSettings>) => {
      setMap((current) => {
        const next = { ...(current[workspaceId] ?? defaults), ...patch };
        void botSettingsApi.set(workspaceId, next).catch(() => {
          // Best-effort persistence; optimistic value stays.
        });
        return { ...current, [workspaceId]: next };
      });
    },
    [],
  );

  return { get, update, loading, reload };
}
