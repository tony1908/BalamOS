import { useCallback, useEffect, useRef, useState } from "react";
import { skillApi } from "../api/skills";
import { message } from "../api/workspaces";
import type { Skill } from "../types";

export interface SkillApiLike {
  list: (workspaceId: string) => Promise<Skill[]>;
  create: (
    workspaceId: string,
    name: string,
    instruction: string,
    enabled: boolean,
  ) => Promise<Skill>;
  update: (
    id: string,
    name: string,
    instruction: string,
    enabled: boolean,
  ) => Promise<Skill>;
  remove: (id: string) => Promise<string>;
}

export function useSkills(workspaceId: string, api: SkillApiLike = skillApi) {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);

  const reload = useCallback(async () => {
    const request = generation.current;
    setLoading(true);
    setError(null);
    try {
      const next = await api.list(workspaceId);
      if (request === generation.current) setSkills(next);
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
    skills,
    loading,
    error,
    create: (name: string, instruction: string, enabled: boolean) =>
      mutate(() => api.create(workspaceId, name, instruction, enabled)),
    update: (id: string, name: string, instruction: string, enabled: boolean) =>
      mutate(() => api.update(id, name, instruction, enabled)),
    remove: (id: string) => mutate(() => api.remove(id)),
    toggle: (skill: Skill) =>
      mutate(() =>
        api.update(skill.id, skill.name, skill.instruction, !skill.enabled),
      ),
    reload,
  };
}
