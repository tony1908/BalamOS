import { useEffect, useRef, useState } from "react";
import { message } from "../api/workspaces";
import { governanceApi, type GovernanceApi } from "../api/governance";
import type {
  GovernancePolicy,
  WorkspaceGovernancePolicy,
  GovernanceView,
  GovernanceRule,
} from "../types";
import "./GovernancePanel.css";

export function GovernancePanel({
  open,
  onClose,
  workspaceId,
  api = governanceApi,
}: {
  open: boolean;
  onClose: () => void;
  workspaceId?: string;
  api?: GovernanceApi;
}) {
  const global = !workspaceId;
  const [, setView] = useState<GovernanceView | null>(null),
    [draft, setDraft] = useState<GovernancePolicy | WorkspaceGovernancePolicy>(
      {} as GovernancePolicy,
    ),
    [library, setLibrary] = useState<GovernanceRule[]>([]);
  const [loading, setLoading] = useState(false),
    [saving, setSaving] = useState(false),
    [error, setError] = useState<string | null>(null),
    [saved, setSaved] = useState(false),
    [reload, setReload] = useState(0);
  const [newTitle, setNewTitle] = useState(""),
    [newBody, setNewBody] = useState(""),
    [editId, setEditId] = useState<string | null>(null),
    [editTitle, setEditTitle] = useState(""),
    [editBody, setEditBody] = useState(""),
    [customTitle, setCustomTitle] = useState(""),
    [customBody, setCustomBody] = useState("");
  const generation = useRef(0),
    dialogRef = useRef<HTMLElement>(null);

  useEffect(() => {
    if (!open) return;
    const gen = ++generation.current;
    setLoading(true);
    setError(null);
    setSaved(false);
    (global ? api.global() : api.workspace(workspaceId!))
      .then((v) => {
        if (generation.current !== gen) return;
        setView(v);
        setDraft(global ? v.global : v.local);
        setLibrary(v.library ?? []);
      })
      .catch((e) => {
        if (generation.current === gen) setError(message(e));
      })
      .finally(() => {
        if (generation.current === gen) setLoading(false);
      });
  }, [api, global, open, reload, workspaceId]);

  useEffect(() => {
    if (!open) return;
    dialogRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;
  const setControl = (key: string, value: boolean) => {
    setDraft((d) => ({ ...d, [key]: value }));
    setSaved(false);
  };
  const applied = (): string[] =>
    global ? [] : ((draft as WorkspaceGovernancePolicy).applied_rule_ids ?? []);
  const customRules = (): GovernanceRule[] =>
    global ? [] : ((draft as WorkspaceGovernancePolicy).custom_rules ?? []);
  const addLibraryRule = async () => {
    if (!newTitle.trim() || !api.createRule) return;
    try {
      await api.createRule(newTitle.trim(), newBody);
      setNewTitle("");
      setNewBody("");
      setReload((r) => r + 1);
    } catch (e) {
      setError(message(e));
    }
  };
  const saveEdit = async () => {
    if (!editId || !api.updateRule) return;
    try {
      await api.updateRule(editId, editTitle.trim(), editBody);
      setEditId(null);
      setReload((r) => r + 1);
    } catch (e) {
      setError(message(e));
    }
  };
  const deleteLibraryRule = async (id: string) => {
    if (!api.deleteRule) return;
    try {
      await api.deleteRule(id);
      if (!global)
        setDraft((d) => ({
          ...d,
          applied_rule_ids: (
            (d as WorkspaceGovernancePolicy).applied_rule_ids ?? []
          ).filter((x) => x !== id),
        }));
      setReload((r) => r + 1);
    } catch (e) {
      setError(message(e));
    }
  };
  const toggleApplied = (id: string) => {
    const cur = applied(),
      next = cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id];
    setDraft((d) => ({ ...d, applied_rule_ids: next }));
    setSaved(false);
  };
  const addCustomRule = () => {
    if (!customTitle.trim()) return;
    const rule = {
      id: `custom-${Date.now()}`,
      title: customTitle.trim(),
      body: customBody,
    };
    setDraft((d) => ({ ...d, custom_rules: [...customRules(), rule] }));
    setCustomTitle("");
    setCustomBody("");
    setSaved(false);
  };
  const removeCustomRule = (id: string) => {
    setDraft((d) => ({
      ...d,
      custom_rules: customRules().filter((r) => r.id !== id),
    }));
    setSaved(false);
  };
  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const next = global
        ? await api.saveGlobal(draft as GovernancePolicy)
        : await api.saveWorkspace(
            workspaceId!,
            draft as WorkspaceGovernancePolicy,
          );
      setView(next);
      setDraft(global ? next.global : next.local);
      setSaved(true);
    } catch (e) {
      setError(message(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className="governance-backdrop"
      role="presentation"
      onMouseDown={(e) => e.currentTarget === e.target && onClose()}
    >
      <section
        className="governance-dialog"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="governance-title"
        tabIndex={-1}
      >
        <div className="governance-head">
          <div>
            <span className="eyebrow">GOVERNANCE</span>
            <h2 id="governance-title">
              {global ? "Global governance" : "Agent governance"}
            </h2>
          </div>
          <button
            className="icon-btn"
            aria-label="Close governance"
            onClick={onClose}
          >
            ×
          </button>
        </div>
        {loading && <p role="status">Loading…</p>}
        <div className="gov-controls">
          <label>
            <input
              type="checkbox"
              checked={(draft as GovernancePolicy).enabled !== false}
              onChange={(e) => setControl("enabled", e.currentTarget.checked)}
            />
            <span>Agent enabled</span>
          </label>
          <label>
            <input
              type="checkbox"
              checked={!!(draft as GovernancePolicy).require_approval}
              onChange={(e) =>
                setControl("require_approval", e.currentTarget.checked)
              }
            />
            <span>Require approval</span>
          </label>
          <label>
            <input
              type="checkbox"
              checked={!!(draft as GovernancePolicy).require_container}
              onChange={(e) =>
                setControl("require_container", e.currentTarget.checked)
              }
            />
            <span>Require container</span>
          </label>
          <label>
            <input
              type="checkbox"
              checked={(draft as GovernancePolicy).allow_scheduled !== false}
              onChange={(e) =>
                setControl("allow_scheduled", e.currentTarget.checked)
              }
            />
            <span>Allow scheduled</span>
          </label>
        </div>
        <p className="governance-note">
          Controls are enforced. Rules below are advisory guidance injected into
          the agent.
        </p>
        <h3>Rule library</h3>
        {library.map((rule) => (
          <div className="gov-rule-card" key={rule.id}>
            {editId === rule.id ? (
              <>
                <label className="ws-field">
                  <span>Title</span>
                  <input
                    value={editTitle}
                    onChange={(e) => setEditTitle(e.currentTarget.value)}
                  />
                </label>
                <label className="ws-field">
                  <span>Instruction</span>
                  <textarea
                    value={editBody}
                    onChange={(e) => setEditBody(e.currentTarget.value)}
                  />
                </label>
                <div className="governance-actions">
                  <button className="pill" onClick={() => setEditId(null)}>
                    Cancel
                  </button>
                  <button
                    className="pill primary"
                    onClick={() => void saveEdit()}
                  >
                    Save rule
                  </button>
                </div>
              </>
            ) : (
              <>
                <strong>{rule.title}</strong>
                <p className="governance-pre">{rule.body}</p>
                <div className="governance-actions">
                  <button
                    className="pill"
                    onClick={() => {
                      setEditId(rule.id);
                      setEditTitle(rule.title);
                      setEditBody(rule.body);
                    }}
                  >
                    Edit
                  </button>
                  <button
                    className="pill danger"
                    onClick={() => void deleteLibraryRule(rule.id)}
                  >
                    Delete
                  </button>
                </div>
              </>
            )}
          </div>
        ))}
        <div className="gov-rule-card">
          <label className="ws-field">
            <span>New rule title</span>
            <input
              aria-label="New rule title"
              value={newTitle}
              onChange={(e) => setNewTitle(e.currentTarget.value)}
            />
          </label>
          <label className="ws-field">
            <span>Instruction</span>
            <textarea
              aria-label="New rule body"
              value={newBody}
              onChange={(e) => setNewBody(e.currentTarget.value)}
            />
          </label>
          <div className="governance-actions">
            <button
              className="pill primary"
              disabled={!newTitle.trim()}
              onClick={() => void addLibraryRule()}
            >
              Add rule
            </button>
          </div>
        </div>
        {!global && (
          <>
            <h3>Applied to this agent</h3>
            {library.map((rule) => (
              <label className="governance-switch" key={rule.id}>
                <input
                  type="checkbox"
                  checked={applied().includes(rule.id)}
                  onChange={() => toggleApplied(rule.id)}
                />
                <span>{rule.title}</span>
              </label>
            ))}
            {customRules().map((rule) => (
              <div className="gov-rule-card" key={rule.id}>
                <strong>{rule.title}</strong>
                <p className="governance-pre">{rule.body}</p>
                <div className="governance-actions">
                  <button
                    className="pill danger"
                    onClick={() => removeCustomRule(rule.id)}
                  >
                    Remove
                  </button>
                </div>
              </div>
            ))}
            <div className="gov-rule-card">
              <label className="ws-field">
                <span>Custom rule title</span>
                <input
                  aria-label="Custom rule title"
                  value={customTitle}
                  onChange={(e) => setCustomTitle(e.currentTarget.value)}
                />
              </label>
              <label className="ws-field">
                <span>Instruction</span>
                <textarea
                  aria-label="Custom rule body"
                  value={customBody}
                  onChange={(e) => setCustomBody(e.currentTarget.value)}
                />
              </label>
              <div className="governance-actions">
                <button
                  className="pill"
                  disabled={!customTitle.trim()}
                  onClick={addCustomRule}
                >
                  Add custom rule
                </button>
              </div>
            </div>
            <p className="governance-note">
              {applied().length + customRules().length} rule(s) will be injected
              into this agent.
            </p>
          </>
        )}
        {error && (
          <p className="governance-error" role="alert">
            {error}
          </p>
        )}
        {saved && (
          <p className="governance-success" role="status">
            Saved.
          </p>
        )}
        <div className="governance-actions">
          <button className="pill" onClick={onClose}>
            Close
          </button>
          <button
            className="pill primary"
            disabled={saving}
            onClick={() => void save()}
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </section>
    </div>
  );
}
