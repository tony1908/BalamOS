import { useCallback, useEffect, useRef, useState } from "react";
import { message, workspaceApi } from "../api/workspaces";
import type {
  Harness,
  PermissionProfile,
  RuntimeStatus,
  Workspace,
} from "../types";

export interface WorkspaceApi {
  list: () => Promise<Workspace[]>;
  create: (
    name: string,
    hostPath: string,
    profile: PermissionProfile,
    harness: Harness,
    model?: string,
    reasoningEffort?: string,
  ) => Promise<Workspace>;
  runtimeStatus: () => Promise<RuntimeStatus>;
  reconcile: () => Promise<RuntimeStatus>;
  start: (id: string) => Promise<Workspace>;
  stop: (id: string) => Promise<Workspace>;
  reset: (id: string) => Promise<Workspace>;
  remove: (id: string) => Promise<string>;
}

export interface WorkspaceController {
  workspaces: Workspace[];
  selectedId: string | null;
  select: (id: string) => void;
  create: (
    name: string,
    hostPath: string,
    profile?: PermissionProfile,
    harness?: Harness,
    model?: string,
    reasoningEffort?: string,
  ) => Promise<Workspace | undefined>;
  start: (id: string) => Promise<void>;
  stop: (id: string) => Promise<void>;
  reset: (id: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  applyUpdate: (row: Workspace) => void;
  loading: boolean;
  error: string | null;
  runtime: RuntimeStatus | null;
  runtimeLoading: boolean;
  retry: () => Promise<void>;
  pending: Record<string, boolean>;
  mutationsReady: boolean;
}

export function useWorkspaces(
  controller: WorkspaceApi = workspaceApi,
): WorkspaceController {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [runtimeLoading, setRuntimeLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);
  const [pending, setPending] = useState<Record<string, boolean>>({});
  const selected = useRef<string | null>(null);
  const pendingIds = useRef(new Set<string>());

  selected.current = selectedId;
  const mutationsReady =
    runtime?.available === true && runtime.mutations_ready === true;

  const loadList = useCallback(async () => {
    const rows = await controller.list();
    setWorkspaces(rows);
    if (!selected.current || !rows.some((row) => row.id === selected.current)) {
      setSelectedId(rows[0]?.id ?? null);
    }
  }, [controller]);

  const retry = useCallback(async () => {
    setError(null);
    setRuntimeLoading(true);
    try {
      const status = await controller.reconcile();
      setRuntime(status);
      if (status.available) {
        await loadList();
      }
    } catch (cause) {
      setError(message(cause));
    } finally {
      setRuntimeLoading(false);
    }
  }, [controller, loadList]);

  useEffect(() => {
    void loadList()
      .catch((cause) => setError(message(cause)))
      .finally(() => setLoading(false));
    void controller
      .runtimeStatus()
      .then(setRuntime)
      .catch((cause) => setError(message(cause)))
      .finally(() => setRuntimeLoading(false));
  }, [controller, loadList]);

  const replace = (row: Workspace) => {
    setWorkspaces((items) =>
      items.some((item) => item.id === row.id)
        ? items.map((item) => (item.id === row.id ? row : item))
        : [...items, row],
    );
  };

  const begin = (id: string) => {
    if (!mutationsReady || pendingIds.current.has(id)) return false;
    pendingIds.current.add(id);
    setPending((value) => ({ ...value, [id]: true }));
    return true;
  };

  const finish = (id: string) => {
    pendingIds.current.delete(id);
    setPending((value) => {
      const next = { ...value };
      delete next[id];
      return next;
    });
  };

  const run = async (id: string, operation: () => Promise<Workspace>) => {
    if (!begin(id)) return;
    setError(null);
    try {
      replace(await operation());
      await loadList();
    } catch (cause) {
      setError(message(cause));
      try {
        await loadList();
      } catch {
        // Preserve the original mutation failure.
      }
      throw cause;
    } finally {
      finish(id);
    }
  };

  const create = async (
    name: string,
    hostPath: string,
    profile: PermissionProfile = "observe",
    harness: Harness = "codex",
    model?: string,
    reasoningEffort?: string,
  ) => {
    if (!mutationsReady) return;
    setError(null);
    try {
      const row = await controller.create(
        name,
        hostPath,
        profile,
        harness,
        model,
        reasoningEffort,
      );
      replace(row);
      setSelectedId(row.id);
      return row;
    } catch (cause) {
      setError(message(cause));
      throw cause;
    }
  };

  const remove = async (id: string) => {
    if (!begin(id)) return;
    setError(null);
    try {
      await controller.remove(id);
      try {
      } catch {
        // Storage may be unavailable.
      }
      setWorkspaces((items) => {
        const index = items.findIndex((item) => item.id === id);
        const next = items.filter((item) => item.id !== id);
        if (selected.current === id) {
          setSelectedId(next[Math.max(0, index - 1)]?.id ?? null);
        }
        return next;
      });
      await loadList();
    } catch (cause) {
      setError(message(cause));
      try {
        await loadList();
      } catch {
        // Preserve the original deletion failure.
      }
      throw cause;
    } finally {
      finish(id);
    }
  };

  return {
    workspaces,
    selectedId,
    select: setSelectedId,
    create,
    start: (id) => run(id, () => controller.start(id)),
    stop: (id) => run(id, () => controller.stop(id)),
    reset: async (id) => {
      await run(id, () => controller.reset(id));
      try {
      } catch {
        // Storage may be unavailable.
      }
    },
    remove,
    applyUpdate: replace,
    loading,
    error,
    runtime,
    runtimeLoading,
    retry,
    pending,
    mutationsReady,
  };
}
