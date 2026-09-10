import { useState } from "react";
import { skillApi } from "../api/skills";
import { type SkillApiLike, useSkills } from "../hooks/useSkills";
import type { Skill } from "../types";

export function SkillsPanel({
  workspaceId,
  api = skillApi,
}: {
  workspaceId: string;
  api?: SkillApiLike;
}) {
  const { skills, loading, error, create, update, remove, toggle, reload } =
    useSkills(workspaceId, api);
  const [editing, setEditing] = useState<Skill | null | false>(false);
  const [name, setName] = useState("");
  const [instruction, setInstruction] = useState("");
  const [enabled, setEnabled] = useState(true);
  const [pending, setPending] = useState(false);

  const openEditor = (skill?: Skill) => {
    setEditing(skill ?? null);
    setName(skill?.name ?? "");
    setInstruction(skill?.instruction ?? "");
    setEnabled(skill?.enabled ?? true);
  };

  async function save(event: React.FormEvent) {
    event.preventDefault();
    if (!name.trim() || !instruction.trim()) return;
    setPending(true);
    try {
      if (editing)
        await update(editing.id, name.trim(), instruction.trim(), enabled);
      else await create(name.trim(), instruction.trim(), enabled);
      setEditing(false);
    } catch {
      // useSkills exposes the existing daemon error in the panel.
    } finally {
      setPending(false);
    }
  }

  return (
    <section className="routines">
      {error && <p role="alert">{error}</p>}
      {error && (
        <button
          className="pill"
          onClick={() => void reload()}
          disabled={loading || pending}
        >
          Retry loading skills
        </button>
      )}
      {!loading && !error && skills.length > 0 && (
        <p role="status">
          Skill changes apply when the next harness session starts. Restart the
          agent or start a new session to load updated instructions.
        </p>
      )}
      {skills.length === 0 && editing === false ? (
        <div className="routine-empty">
          <p>Skills are always-on capabilities this bot uses.</p>
          <button className="pill primary" onClick={() => openEditor()}>
            Create Skill
          </button>
        </div>
      ) : (
        <>
          <h2>Skills</h2>
          <p>Always-on capabilities this bot uses.</p>
          <div className="routine-list">
            {skills.map((skill) => (
              <div className="routine-row" key={skill.id}>
                <div>
                  <strong>{skill.name}</strong>
                </div>
                <label>
                  <input
                    type="checkbox"
                    aria-label={skill.name}
                    checked={skill.enabled}
                    onChange={() => {
                      setPending(true);
                      void toggle(skill)
                        .catch(() => undefined)
                        .finally(() => setPending(false));
                    }}
                    disabled={loading || pending}
                  />{" "}
                  Enabled
                </label>
                <button
                  className="pill"
                  disabled={loading || pending}
                  onClick={() => openEditor(skill)}
                >
                  Edit
                </button>
                <button
                  className="pill"
                  disabled={loading || pending}
                  onClick={() => {
                    setPending(true);
                    void remove(skill.id)
                      .catch(() => undefined)
                      .finally(() => setPending(false));
                  }}
                >
                  Delete
                </button>
              </div>
            ))}
          </div>
          {editing === false && (
            <button className="pill primary" onClick={() => openEditor()}>
              Create Skill
            </button>
          )}
        </>
      )}
      {editing !== false && (
        <form className="routine-editor" onSubmit={save}>
          <label className="ws-field">
            <span>Name</span>
            <input
              aria-label="Name"
              value={name}
              onChange={(event) => setName(event.currentTarget.value)}
            />
          </label>
          <label className="ws-field">
            <span>Instruction</span>
            <textarea
              aria-label="Instruction"
              value={instruction}
              onChange={(event) => setInstruction(event.currentTarget.value)}
            />
          </label>
          <label>
            <input
              type="checkbox"
              checked={enabled}
              onChange={(event) => setEnabled(event.currentTarget.checked)}
            />{" "}
            Enabled
          </label>
          <div className="routine-actions">
            <button
              className="pill primary"
              type="submit"
              disabled={!name.trim() || !instruction.trim() || pending}
            >
              Save
            </button>
            <button
              className="pill"
              type="button"
              onClick={() => setEditing(false)}
            >
              Cancel
            </button>
          </div>
        </form>
      )}
    </section>
  );
}
