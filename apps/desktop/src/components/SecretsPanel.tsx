import { useEffect, useRef, useState } from "react";
import { secretsApi, type SecretsApi } from "../api/secrets";
import type { SecretMetadata } from "../types";

type Props = {
  open: boolean;
  onClose: () => void;
  workspaceId?: string;
  api?: SecretsApi;
};

export function SecretsPanel({
  open,
  onClose,
  workspaceId,
  api = secretsApi,
}: Props) {
  const [secrets, setSecrets] = useState<SecretMetadata[]>([]);
  const [assigned, setAssigned] = useState<Set<string>>(new Set());
  const [name, setName] = useState("");
  const [env, setEnv] = useState("");
  const [addValue, setAddValue] = useState("");
  const [replaceValue, setReplaceValue] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);
  const dialog = useRef<HTMLElement>(null);
  const previouslyFocused = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) {
      generation.current += 1;
      setSecrets([]);
      setAssigned(new Set());
      setSelected(null);
      setAddValue("");
      setReplaceValue("");
      setLoading(false);
      setBusy(false);
      setError(null);
      return;
    }
    const current = ++generation.current;
    setSecrets([]);
    setAssigned(new Set());
    setLoading(true);
    setBusy(false);
    setError(null);
    setSelected(null);
    setAddValue("");
    setReplaceValue("");
    Promise.all([
      api.list(),
      workspaceId ? api.assignments(workspaceId) : Promise.resolve([]),
    ])
      .then(([rows, links]) => {
        if (generation.current !== current) return;
        setSecrets(rows);
        setAssigned(new Set(links.map((link) => link.secret_id)));
      })
      .catch((cause) => {
        if (generation.current === current)
          setError(
            cause instanceof Error ? cause.message : "Unable to load secrets",
          );
      })
      .finally(() => {
        if (generation.current === current) setLoading(false);
      });
    return () => {
      generation.current += 1;
    };
  }, [api, open, workspaceId]);

  useEffect(() => {
    if (!open) return;
    previouslyFocused.current = document.activeElement as HTMLElement | null;
    dialog.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== "Tab" || !dialog.current) return;
      const focusable = Array.from(
        dialog.current.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      );
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      previouslyFocused.current?.focus();
    };
  }, [onClose, open]);

  if (!open) return null;
  const disabled = loading || busy;
  const act = async (operation: () => Promise<unknown>, after?: () => void) => {
    if (disabled) return;
    const current = generation.current;
    setBusy(true);
    setError(null);
    try {
      await operation();
      if (generation.current !== current) return;
      after?.();
      const rows = await api.list();
      if (generation.current !== current) return;
      setSecrets(rows);
    } catch (cause) {
      if (generation.current === current)
        setError(
          cause instanceof Error ? cause.message : "Secret operation failed",
        );
    } finally {
      if (generation.current === current) setBusy(false);
    }
  };
  return (
    <div
      className="governance-backdrop"
      role="presentation"
      onMouseDown={(event) => event.currentTarget === event.target && onClose()}
    >
      <section
        className="governance-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="secrets-title"
        tabIndex={-1}
        ref={dialog}
      >
        <div className="governance-head">
          <div>
            <span className="eyebrow">SECRETS</span>
            <h2 id="secrets-title">Secrets</h2>
          </div>
          <button className="icon-btn" aria-label="Close secrets" onClick={onClose}>×</button>
        </div>
        <p className="governance-sub">
          Stored in the OS credential store.{workspaceId ? " Assigned agents can read them;" : ""} updates apply to new sessions.
        </p>
        {error && <p className="governance-error" role="alert">{error}</p>}

        <section className="gov-section">
          <header className="gov-section-head">
            <h3>Stored secrets</h3>
            <p>{workspaceId ? "Toggle which secrets this agent can read." : "Credentials available to assign to agents."}</p>
          </header>
          {loading ? (
            <p role="status">Loading…</p>
          ) : secrets.length === 0 ? (
            <p className="governance-note">No secrets yet.</p>
          ) : (
            secrets.map((secret) => (
              <div className="gov-rule-card" key={secret.id}>
                <div className="gov-rule-top">
                  <span className="secret-id">
                    <strong>{secret.name}</strong>
                    <small>{secret.env_name}</small>
                  </span>
                  {workspaceId && (
                    <label className="gov-apply">
                      <input
                        type="checkbox"
                        checked={assigned.has(secret.id)}
                        disabled={disabled}
                        onChange={() =>
                          void act(
                            () =>
                              assigned.has(secret.id)
                                ? api.unassign(secret.id, workspaceId)
                                : api.assign(secret.id, workspaceId),
                            () =>
                              setAssigned((current) => {
                                const next = new Set(current);
                                if (next.has(secret.id)) next.delete(secret.id);
                                else next.add(secret.id);
                                return next;
                              }),
                          )
                        }
                      />
                      <span>Assign</span>
                    </label>
                  )}
                </div>
                {selected === secret.id && (
                  <label className="ws-field">
                    <span>Replacement password</span>
                    <input
                      disabled={disabled}
                      aria-label="Replacement password"
                      type="password"
                      value={replaceValue}
                      onChange={(event) => setReplaceValue(event.target.value)}
                    />
                  </label>
                )}
                <div className="governance-actions">
                  {selected === secret.id ? (
                    <>
                      <button className="pill" disabled={disabled} onClick={() => { setSelected(null); setReplaceValue(""); }}>Cancel</button>
                      <button
                        className="pill primary"
                        disabled={disabled || !replaceValue}
                        onClick={() =>
                          void act(
                            () => api.replace(secret.id, replaceValue),
                            () => { setReplaceValue(""); setSelected(null); },
                          )
                        }
                      >
                        Replace value
                      </button>
                    </>
                  ) : (
                    <button
                      className="pill"
                      disabled={disabled}
                      onClick={() => { setSelected(secret.id); setReplaceValue(""); }}
                    >
                      Replace
                    </button>
                  )}
                  <button className="pill danger" disabled={disabled} onClick={() => void act(() => api.remove(secret.id))}>Delete</button>
                </div>
              </div>
            ))
          )}
        </section>

        <section className="gov-section">
          <header className="gov-section-head">
            <h3>Add secret</h3>
            <p>Create a new credential.</p>
          </header>
          <form
            className="secret-add"
            onSubmit={(event) => {
              event.preventDefault();
              if (name.trim() && env.trim() && addValue)
                void act(
                  () => api.create(name.trim(), env.trim(), addValue),
                  () => { setName(""); setEnv(""); setAddValue(""); },
                );
            }}
          >
            <label className="ws-field">
              <span>Name</span>
              <input disabled={disabled} aria-label="Name" value={name} onChange={(event) => setName(event.target.value)} />
            </label>
            <label className="ws-field">
              <span>Environment name</span>
              <input disabled={disabled} aria-label="Environment name" value={env} onChange={(event) => setEnv(event.target.value)} />
            </label>
            <label className="ws-field">
              <span>Password value</span>
              <input disabled={disabled} aria-label="Password value" type="password" value={addValue} onChange={(event) => setAddValue(event.target.value)} />
            </label>
            <div className="governance-actions">
              <button className="pill primary" disabled={disabled || !name.trim() || !env.trim() || !addValue}>Add</button>
            </div>
          </form>
        </section>
      </section>
    </div>
  );
}
