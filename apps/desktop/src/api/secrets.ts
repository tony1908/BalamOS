import { invoke } from "@tauri-apps/api/core";
import type { SecretAssignment, SecretMetadata } from "../types";

export type SecretsApi = {
  list: () => Promise<SecretMetadata[]>;
  create: (name: string, envName: string, value: string) => Promise<SecretMetadata>;
  replace: (id: string, value: string) => Promise<void>;
  remove: (id: string) => Promise<string>;
  assignments: (workspaceId: string) => Promise<SecretAssignment[]>;
  assign: (secretId: string, workspaceId: string) => Promise<SecretAssignment>;
  unassign: (secretId: string, workspaceId: string) => Promise<void>;
};
export const secretsApi: SecretsApi = {
  list: () => invoke<SecretMetadata[]>("list_secrets"),
  create: (name, envName, value) => invoke<SecretMetadata>("create_secret", { name, envName, value }),
  replace: (id, value) => invoke<void>("replace_secret", { id, value }),
  remove: (id) => invoke<string>("delete_secret", { id }),
  assignments: (workspaceId) => invoke<SecretAssignment[]>("list_secret_assignments", { workspaceId }),
  assign: (secretId, workspaceId) => invoke<SecretAssignment>("assign_secret", { secretId, workspaceId }),
  unassign: (secretId, workspaceId) => invoke<void>("unassign_secret", { secretId, workspaceId }),
};
