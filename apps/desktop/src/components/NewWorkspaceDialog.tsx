import { useEffect, useRef, useState } from "react";
import type { Harness, ModelOption, PermissionProfile, Template } from "../types";
import { message, workspaceApi } from "../api/workspaces";
import { CUSTOM_MODEL, ModelSelect, REASONING_EFFORTS } from "./workspacePresentation";

type Props = {
  open: boolean;
  mutationsReady: boolean;
  onCreate: (
    name: string,
    hostPath: string,
    profile: PermissionProfile,
    harness: Harness,
    template?: Template,
    avatarColor?: string,
    model?: string,
    reasoningEffort?: string,
  ) => Promise<void>;
  onClose: () => void;
  templates?: Template[];
};

const PROFILES: { value: PermissionProfile; label: string }[] = [
  { value: "workspace", label: "Workspace" },
  { value: "observe", label: "Observe" },
  { value: "full_control", label: "Full control" },
];
const HARNESSES: { value: Harness; label: string }[] = [
  { value: "codex", label: "Codex" },
  { value: "opencode", label: "opencode" },
  { value: "claude_code", label: "Claude Code" },
  { value: "antigravity", label: "Antigravity" },
];
const MASCOT_COLORS = [
  "#529e85",
  "#db806e",
  "#6b9bd2",
  "#c89b55",
  "#a984d0",
  "#5bb0a8",
  "#e0a54a",
  "#cc6b8e",
];

export function NewWorkspaceDialog({
  open,
  mutationsReady,
  onCreate,
  onClose,
  templates = [],
}: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState("");
  const [hostPath, setHostPath] = useState("");
  const [profile, setProfile] = useState<PermissionProfile>("workspace");
  const [harness, setHarness] = useState<Harness>("codex");
  const [templateId, setTemplateId] = useState("");
  const [avatarColor, setAvatarColor] = useState(MASCOT_COLORS[0]);
  const [model, setModel] = useState("");
  const [customModel, setCustomModel] = useState("");
  const [models, setModels] = useState<ModelOption[]>([]);
  const [reasoningEffort, setReasoningEffort] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    if (open && !node.open) node.showModal();
    if (!open && node.open) node.close();
  }, [open]);

  useEffect(() => {
    let live = true;
    workspaceApi
      .listModels(harness)
      .then((m) => {
        if (live) setModels(m);
      })
      .catch(() => {
        if (live) setModels([]);
      });
    return () => {
      live = false;
    };
  }, [harness]);

  if (!open) return null;

  const canSubmit = mutationsReady && !submitting && name.trim() !== "";

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!canSubmit) return;
    setSubmitting(true);
    setError(null);
    try {
      const template = templates.find((t) => t.id === templateId);
      const finalModel = model === CUSTOM_MODEL ? customModel.trim() : model;
      await onCreate(
        name.trim(),
        hostPath.trim(),
        profile,
        harness,
        template,
        avatarColor,
        finalModel,
        reasoningEffort,
      );
      setName("");
      setHostPath("");
      setProfile("workspace");
      setHarness("codex");
      setModel("");
      setCustomModel("");
      setModels([]);
      setReasoningEffort("");
      onClose();
    } catch (cause) {
      setError(message(cause));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <dialog
      className="ws-dialog"
      ref={ref}
      aria-label="New agent"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <form className="ws-form" onSubmit={submit}>
        <h2 className="ws-form-title">New agent</h2>
        {templates.length > 0 && (
          <label className="ws-field">
            <span>Start from template</span>
            <select
              aria-label="Start from template"
              value={templateId}
              onChange={(event) => setTemplateId(event.currentTarget.value)}
            >
              <option value="">None (blank bot)</option>
              {templates.map((template) => (
                <option key={template.id} value={template.id}>
                  {template.name}
                </option>
              ))}
            </select>
          </label>
        )}
        <label className="ws-field">
          <span className="ws-field-label">
            Name <span aria-hidden="true">*</span>
          </span>
          <input
            aria-label="Name"
            value={name}
            onChange={(event) => setName(event.currentTarget.value)}
            autoFocus
          />
        </label>
        <label className="ws-field">
          <span>Mascot color</span>
          <div className="swatches">
            {MASCOT_COLORS.map((c) => (
              <button
                type="button"
                key={c}
                className={`swatch${avatarColor === c ? " selected" : ""}`}
                style={{ background: c }}
                aria-label={`Mascot color ${c}`}
                aria-pressed={avatarColor === c}
                onClick={() => setAvatarColor(c)}
              />
            ))}
          </div>
        </label>
        <label className="ws-field">
          <span>Harness</span>
          <select
            value={harness}
            onChange={(event) =>
              setHarness(event.currentTarget.value as Harness)
            }
          >
            {HARNESSES.map((item) => (
              <option key={item.value} value={item.value}>
                {item.label}
              </option>
            ))}
          </select>
        </label>
        <label className="ws-field">
          <span>Model</span>
          <ModelSelect
            value={model}
            customValue={customModel}
            models={models}
            onChange={setModel}
            onCustomChange={setCustomModel}
          />
          <span className="ws-field-hint">
            Default uses the harness's built-in model.
          </span>
        </label>
        <label className="ws-field">
          <span>Reasoning effort</span>
          <select
            value={reasoningEffort}
            onChange={(event) => setReasoningEffort(event.currentTarget.value)}
          >
            {REASONING_EFFORTS.map((item) => (
              <option key={item.value} value={item.value}>
                {item.label}
              </option>
            ))}
          </select>
        </label>
        <label className="ws-field">
          <span className="ws-field-label">Project folder</span>
          <input
            aria-label="Project folder"
            value={hostPath}
            onChange={(event) => setHostPath(event.currentTarget.value)}
            placeholder="/projects/my-app"
          />
          <span className="ws-field-hint">
            Optional — leave blank to store this bot under a managed folder
            (~/Orbit/workspaces/&lt;name&gt;). Or enter an absolute path on your
            computer to mount at /workspace.
          </span>
        </label>
        <label className="ws-field">
          <span>Safety profile</span>
          <select
            value={profile}
            onChange={(event) =>
              setProfile(event.currentTarget.value as PermissionProfile)
            }
          >
            {PROFILES.map((item) => (
              <option key={item.value} value={item.value}>
                {item.label}
              </option>
            ))}
          </select>
        </label>
        {!mutationsReady && (
          <p className="ws-form-note" role="status">
            Workspace creation is available once the daemon is ready.
          </p>
        )}
        {mutationsReady && !submitting && name.trim() === "" && (
          <p className="ws-form-note ws-field-hint" role="note">
            Enter a name to continue.
          </p>
        )}
        {error && (
          <p className="ws-form-error" role="alert">
            {error}
          </p>
        )}
        <div className="ws-form-actions">
          <button
            type="button"
            className="pill"
            onClick={onClose}
            disabled={submitting}
          >
            Cancel
          </button>
          <button type="submit" className="pill primary" disabled={!canSubmit}>
            {submitting ? "Adding…" : "Add agent"}
          </button>
        </div>
      </form>
    </dialog>
  );
}
