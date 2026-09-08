import { useCallback, useEffect, useRef, useState } from "react";
import { routineApi } from "../api/routines";
import { message } from "../api/workspaces";
import type { Routine } from "../types";

export interface RoutineApiLike {
  list: (workspaceId: string) => Promise<Routine[]>;
  create: (
    workspaceId: string,
    name: string,
    instruction: string,
    intervalMinutes: number,
    enabled: boolean,
  ) => Promise<Routine>;
  update: (
    id: string,
    name: string,
    instruction: string,
    intervalMinutes: number,
    enabled: boolean,
  ) => Promise<Routine>;
  remove: (id: string) => Promise<string>;
}

export function useRoutines(
  workspaceId: string,
  api: RoutineApiLike = routineApi,
) {
  const [routines, setRoutines] = useState<Routine[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);

  const reload = useCallback(async () => {
    const request = generation.current;
    setLoading(true);
    setError(null);
    try {
      const next = await api.list(workspaceId);
      if (request === generation.current) setRoutines(next);
    } catch (cause) {
      if (request === generation.current) setError(message(cause));
    } finally {
      if (request === generation.current) setLoading(false);
    }
  }, [api, workspaceId]);

  useEffect(() => {
    generation.current += 1;
    void reload();
    return () => {
      generation.current += 1;
    };
  }, [reload]);

  const mutate = async (operation: () => Promise<unknown>) => {
    setError(null);
    try {
      await operation();
      await reload();
    } catch (cause) {
      setError(message(cause));
      throw cause;
    }
  };

  return {
    routines,
    loading,
    error,
    create: (
      name: string,
      instruction: string,
      intervalMinutes: number,
      enabled: boolean,
    ) =>
      mutate(() =>
        api.create(workspaceId, name, instruction, intervalMinutes, enabled),
      ),
    update: (
      id: string,
      name: string,
      instruction: string,
      intervalMinutes: number,
      enabled: boolean,
    ) =>
      mutate(() => api.update(id, name, instruction, intervalMinutes, enabled)),
    remove: (id: string) => mutate(() => api.remove(id)),
    toggle: (routine: Routine) =>
      mutate(() =>
        api.update(
          routine.id,
          routine.name,
          routine.instruction,
          routine.interval_minutes,
          !routine.enabled,
        ),
      ),
    reload,
  };
}
