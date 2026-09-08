import type {
  AgentSessionState,
  LifecycleState,
  ModelOption,
  PermissionProfile,
  Workspace,
} from "../types";

export const STATE_TEXT: Record<LifecycleState, string> = {
  creating: "creating",
  starting: "starting",
  running: "running",
  stopping: "stopping",
  stopped: "stopped",
  resetting: "resetting",
  deleting: "deleting",
  deleted: "deleted",
  failed: "degraded",
};

const LIFECYCLE_STATUS: Record<LifecycleState, string> = {
  creating: "creating…",
  starting: "waking…",
  running: "active",
  stopping: "pausing…",
  stopped: "asleep",
  resetting: "resetting…",
  deleting: "removing…",
  deleted: "removed",
  failed: "needs attention",
};

const AGENT_STATUS: Record<AgentSessionState, string> = {
  starting: "connecting…",
  needs_authentication: "needs sign-in",
  idle: "idle",
  running: "working…",
  waiting_for_approval: "waiting on you",
  interrupted: "stopped",
  completed: "done",
  failed: "agent failed",
};

export function botStatus(
  lifecycle: LifecycleState,
  agent?: AgentSessionState,
): string {
  if (agent) return AGENT_STATUS[agent];
  return LIFECYCLE_STATUS[lifecycle];
}

const DOT: Record<
  LifecycleState,
  "running" | "waiting" | "failed" | "stopped"
> = {
  creating: "waiting",
  starting: "waiting",
  running: "running",
  stopping: "waiting",
  stopped: "stopped",
  resetting: "waiting",
  deleting: "waiting",
  deleted: "stopped",
  failed: "failed",
};

export const PROFILE_LABEL: Record<PermissionProfile, string> = {
  observe: "Observe",
  workspace: "Workspace",
  full_control: "Full control",
};

export const REASONING_EFFORTS: { value: string; label: string }[] = [
  { value: "", label: "Default" },
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
];

// Sentinel select value that reveals the free-text fallback input.
export const CUSTOM_MODEL = "__custom__";

// Shared "dropdown with custom" control for picking a harness model: a
// <select> of fetched options plus a "Default" and "Custom…" entry, backed
// by a free-text input when Custom is chosen. The caller owns the label,
// state, and submit wiring.
export function ModelSelect({
  value,
  customValue,
  models,
  onChange,
  onCustomChange,
}: {
  value: string;
  customValue: string;
  models: ModelOption[];
  onChange: (value: string) => void;
  onCustomChange: (value: string) => void;
}) {
  return (
    <>
      <select
        aria-label="Model"
        value={value}
        onChange={(event) => onChange(event.currentTarget.value)}
      >
        <option value="">Default</option>
        {models.map((option) => (
          <option key={option.id} value={option.id}>
            {option.label}
          </option>
        ))}
        <option value={CUSTOM_MODEL}>Custom…</option>
      </select>
      {value === CUSTOM_MODEL && (
        <input
          aria-label="Custom model"
          value={customValue}
          onChange={(event) => onCustomChange(event.currentTarget.value)}
          placeholder="Model id"
        />
      )}
    </>
  );
}

// A compact, human-friendly host path for display. Disposable/temp mounts
// (e.g. /private/var/folders/.../T/orbit-disposable-...) collapse to just the
// folder name; the home directory collapses to ~. The full path is kept as a
// tooltip at the call site.
export function prettyPath(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  if (!trimmed) return path;
  if (/\/var\/folders\/|(^|\/)(private\/)?tmp\//.test(trimmed)) {
    return trimmed.split("/").pop() || trimmed;
  }
  const home = trimmed.match(/^\/(?:Users|home)\/[^/]+(\/.*)?$/);
  if (home) return "~" + (home[1] ?? "");
  return trimmed;
}

// Presentation-only: a deterministic colour derived from workspace identity.
function avatarColor(seed: string): string {
  let hash = 0;
  for (let index = 0; index < seed.length; index += 1) {
    hash = (hash * 31 + seed.charCodeAt(index)) >>> 0;
  }
  return `hsl(${hash % 360} 40% 52%)`;
}

export function Avatar({
  workspace,
  size,
  color,
}: {
  workspace: Workspace;
  size?: number;
  color?: string;
}) {
  const dimension = size ?? 40;
  return (
    <span
      className="avatar"
      aria-hidden="true"
      style={{
        background: color ?? avatarColor(workspace.id),
        width: dimension,
        height: dimension,
        fontSize: dimension * 0.38,
      }}
    >
      <span className="avatar-eye" />
      <span className="avatar-eye" />
      <span className={`sd ${DOT[workspace.state]}`} />
    </span>
  );
}
