export type LifecycleState =
  | "creating"
  | "starting"
  | "running"
  | "stopping"
  | "stopped"
  | "resetting"
  | "deleting"
  | "deleted"
  | "failed";
export type PermissionProfile = "observe" | "workspace" | "full_control";
export type Harness =
  "codex" | "opencode" | "claude_code" | "antigravity";
export type GovernancePolicy = { enabled: boolean; require_approval: boolean; require_container: boolean; allow_scheduled: boolean; guidance: string };
export type GovernanceRule = { id: string; title: string; body: string };
export type WorkspaceGovernancePolicy = { enabled?: boolean | null; require_approval?: boolean | null; require_container?: boolean | null; allow_scheduled?: boolean | null; guidance?: string | null; applied_rule_ids?: string[] | null; custom_rules?: GovernanceRule[] | null };
export type GovernanceSource = "global" | "local" | "both" | "default";
export type EffectiveGovernancePolicy = GovernancePolicy & { sources: Record<string, GovernanceSource> };
export type GovernanceBlockedReason = "Disabled" | "ContainerRequired" | "HarnessUnsupportedApproval" | "ScheduledRunsDisabled";
export type GovernanceView = { global: GovernancePolicy; local: WorkspaceGovernancePolicy; effective: EffectiveGovernancePolicy; blocked_reasons: GovernanceBlockedReason[]; library: GovernanceRule[] };
export type GovernancePreset = { id: string; name: string; policy: GovernancePolicy; builtin: boolean };
export type SecretMetadata = { id: string; name: string; env_name: string };
export type SecretAssignment = { secret_id: string; workspace_id: string };
export type ModelOption = { id: string; label: string };
export type AppOption = {
  id: string;
  label: string;
  description: string;
};
export type ResourceLimits = {
  cpus: number;
  memory_bytes: number;
  pids: number;
  soft_disk_bytes: number;
};
export type Workspace = {
  id: string;
  name: string;
  host_path: string;
  profile: PermissionProfile;
  harness: Harness;
  model?: string | null;
  reasoning_effort?: string | null;
  apps?: string[];
  resources: ResourceLimits;
  state: LifecycleState;
};
export type RoutineRunStatus = "ok" | "failed" | "skipped";
export interface Routine {
  id: string;
  workspace_id: string;
  name: string;
  instruction: string;
  interval_minutes: number;
  enabled: boolean;
  last_run_unix_ms?: number | null;
  last_status?: RoutineRunStatus | null;
  last_skip_reason?: string | null;
}
export interface Skill {
  id: string;
  workspace_id: string;
  name: string;
  instruction: string;
  enabled: boolean;
}
export interface TemplateSettings {
  display_name?: string | null;
  label?: string | null;
  description?: string | null;
  notifications: boolean;
}
export interface TemplateRoutine {
  name: string;
  instruction: string;
  interval_minutes: number;
  enabled: boolean;
}
export interface TemplateSkill {
  name: string;
  instruction: string;
  enabled: boolean;
}
export interface Template {
  id: string;
  name: string;
  settings: TemplateSettings;
  routines: TemplateRoutine[];
  skills: TemplateSkill[];
}
export type RuntimeStatus = {
  available: boolean;
  mutations_ready: boolean;
  version?: string | null;
  message: string;
};
export type DesktopSession = { url: string; expires_at_unix_ms: number };
export type CommandError = {
  code: string;
  message: string;
  retryable: boolean;
};
export type AgentSessionState =
  | "starting"
  | "needs_authentication"
  | "idle"
  | "running"
  | "waiting_for_approval"
  | "interrupted"
  | "completed"
  | "failed";
export type ApprovalDecision =
  "accept" | "accept_for_session" | "decline" | "cancel";
export type AgentPlanStep = {
  text: string;
  state: "pending" | "in_progress" | "completed";
};
export type AgentEventKind =
  | { type: "user_prompt"; data: { text: string } }
  | { type: "state_changed"; data: { state: AgentSessionState } }
  | {
      type: "authentication_required";
      data: { url: string; user_code?: string | null };
    }
  | { type: "assistant_delta"; data: { text: string } }
  | {
      type: "plan_updated";
      data: { explanation?: string | null; steps: AgentPlanStep[] };
    }
  | {
      type: "tool_started";
      data: { item_id: string; title: string; detail?: string | null };
    }
  | {
      type: "tool_completed";
      data: { item_id: string; success: boolean; detail?: string | null };
    }
  | {
      type: "approval_requested";
      data: {
        request_id: string;
        kind: "command" | "file_change";
        title: string;
        detail?: string | null;
      };
    }
  | {
      type: "approval_resolved";
      data: { request_id: string; decision: ApprovalDecision };
    }
  | {
      type: "usage_updated";
      data: {
        input_tokens: number;
        cached_input_tokens: number;
        output_tokens: number;
      };
    }
  | {
      type: "error";
      data: { code: string; message: string; retryable: boolean };
    };
export type AgentEvent = { cursor: number; kind: AgentEventKind };
export type AgentEventBatch = {
  workspace_id: string;
  after: number;
  next_cursor: number;
  events: AgentEvent[];
  has_more?: boolean;
};
export type AgentSession = {
  workspace_id: string;
  thread_id?: string | null;
  state: AgentSessionState;
  next_cursor: number;
};
