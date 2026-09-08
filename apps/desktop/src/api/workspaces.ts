import { invoke } from "@tauri-apps/api/core";
import type {
  DesktopSession,
  Harness,
  AppOption,
  ModelOption,
  PermissionProfile,
  RuntimeStatus,
  Workspace,
} from "../types";

export const workspaceApi = {
  list: () => invoke<Workspace[]>("list_workspaces"),
  create: (
    name: string,
    hostPath: string,
    profile: PermissionProfile,
    harness: Harness,
    model?: string,
    reasoningEffort?: string,
  ) =>
    invoke<Workspace>("create_workspace", {
      name,
      hostPath,
      profile,
      harness,
      model: model || null,
      reasoningEffort: reasoningEffort || null,
    }),
  runtimeStatus: () => invoke<RuntimeStatus>("runtime_status"),
  reconcile: () => invoke<RuntimeStatus>("reconcile_runtime"),
  start: (id: string) => invoke<Workspace>("start_workspace", { id }),
  stop: (id: string) => invoke<Workspace>("stop_workspace", { id }),
  reset: (id: string) => invoke<Workspace>("reset_workspace", { id }),
  remove: (id: string) => invoke<string>("delete_workspace", { id }),
  createDesktopSession: (id: string) =>
    invoke<DesktopSession>("create_desktop_session", { id }),
  listModels: (harness: Harness) =>
    invoke<ModelOption[]>("list_models", { harness }),
  setWorkspaceModel: (
    workspaceId: string,
    model: string,
    reasoningEffort: string,
  ) =>
    invoke<Workspace>("set_workspace_model", {
      workspaceId,
      model: model || null,
      reasoningEffort: reasoningEffort || null,
    }),
  listApps: () => invoke<AppOption[]>("list_apps"),
  setWorkspaceApps: (workspaceId: string, apps: string[]) =>
    invoke<Workspace>("set_workspace_apps", { workspaceId, apps }),
};

export const message = (error: unknown) => {
  const text =
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof error.message === "string"
      ? error.message
      : error instanceof Error
        ? error.message
        : String(error);
  // A failed Tauri `invoke` outside the native app (e.g. a plain browser or
  // Vite preview) throws a cryptic "reading 'invoke'" TypeError because the
  // Tauri IPC bridge is absent. Turn it into an actionable message.
  if (/reading 'invoke'|__TAURI_INTERNALS__/.test(text)) {
    return "Orbit's backend runs in the native desktop app. A browser preview cannot reach the local daemon — launch the Orbit desktop app.";
  }
  return text;
};
