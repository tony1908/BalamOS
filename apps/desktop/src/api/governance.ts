import { invoke } from "@tauri-apps/api/core";
import type { GovernancePolicy, GovernancePreset, GovernanceRule, GovernanceView, WorkspaceGovernancePolicy } from "../types";

export type GovernanceApi = {
  global: () => Promise<GovernanceView>;
  saveGlobal: (policy: GovernancePolicy) => Promise<GovernanceView>;
  workspace: (workspaceId: string) => Promise<GovernanceView>;
  saveWorkspace: (workspaceId: string, policy: WorkspaceGovernancePolicy) => Promise<GovernanceView>;
  listPresets?: () => Promise<GovernancePreset[]>;
  createPreset?: (name: string, policy: GovernancePolicy) => Promise<GovernancePreset>;
  deletePreset?: (id: string) => Promise<string>;
  listRules?: () => Promise<GovernanceRule[]>;
  createRule?: (title: string, body: string) => Promise<GovernanceRule>;
  updateRule?: (id: string, title: string, body: string) => Promise<GovernanceRule>;
  deleteRule?: (id: string) => Promise<string>;
};

export const governanceApi: GovernanceApi = {
  global: () => invoke<GovernanceView>("get_governance_policy"),
  saveGlobal: (policy) => invoke<GovernanceView>("save_governance_policy", { policy }),
  workspace: (workspaceId) => invoke<GovernanceView>("get_workspace_governance", { workspaceId }),
  saveWorkspace: (workspaceId, policy) => invoke<GovernanceView>("save_workspace_governance", { workspaceId, policy }),
  listPresets: () => invoke<GovernancePreset[]>("list_governance_presets"),
  createPreset: (name, policy) => invoke<GovernancePreset>("create_governance_preset", { name, policy }),
  deletePreset: (id) => invoke<string>("delete_governance_preset", { id }),
  listRules: () => invoke<GovernanceRule[]>("list_governance_rules"),
  createRule: (title, body) => invoke<GovernanceRule>("create_governance_rule", { title, body }),
  updateRule: (id, title, body) => invoke<GovernanceRule>("update_governance_rule", { id, title, body }),
  deleteRule: (id) => invoke<string>("delete_governance_rule", { id }),
};
