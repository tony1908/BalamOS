import { useState } from "react";
import { routineApi } from "../api/routines";
import type { Routine } from "../types";
import { type RoutineApiLike, useRoutines } from "../hooks/useRoutines";

const CADENCES = [
  { minutes: 15, label: "Every 15 min" },
  { minutes: 30, label: "Every 30 min" },
  { minutes: 60, label: "Every hour" },
  { minutes: 360, label: "Every 6 hours" },
  { minutes: 1440, label: "Every day" },
];

export function cadenceLabel(minutes: number) {
  return (
    CADENCES.find((cadence) => cadence.minutes === minutes)?.label ??
    `Every ${minutes} min`
  );
}

export function RoutinesPanel({
  workspaceId,
  api = routineApi,
}: {
  workspaceId: string;
  api?: RoutineApiLike;
}) {
  const { routines, error, create, update, remove, toggle } = useRoutines(
    workspaceId,
    api,
  );
  const [editing, setEditing] = useState<Routine | null | false>(false);
  const [name, setName] = useState("");
  const [instruction, setInstruction] = useState("");
  const [intervalMinutes, setIntervalMinutes] = useState(15);
  const [enabled, setEnabled] = useState(true);

  const openEditor = (routine?: Routine) => {
    setEditing(routine ?? null);
    setName(routine?.name ?? "");
    setInstruction(routine?.instruction ?? "");
    setIntervalMinutes(routine?.interval_minutes ?? 15);
    setEnabled(routine?.enabled ?? true);
  };

  async function save(event: React.FormEvent) {
    event.preventDefault();
    if (!name.trim() || !instruction.trim()) return;
    if (editing)
      await update(
        editing.id,
        name.trim(),
        instruction.trim(),
        intervalMinutes,
        enabled,
      );
    else
      await create(name.trim(), instruction.trim(), intervalMinutes, enabled);
    setEditing(false);
  }

  return (
    <section className="routines">
      {error && <p role="alert">{error}</p>}
      {routines.length === 0 && editing === false ? (
        <div className="routine-empty">
          <p>Routines are recurring tasks this bot runs on a schedule.</p>
          <button className="pill primary" onClick={() => openEditor()}>
            Create Routine
          </button>
        </div>
      ) : (
        <>
          <h2>Routines</h2>
          <p>Recurring tasks this bot runs on a schedule.</p>
          <div className="routine-list">
            {routines.map((routine) => (
              <div className="routine-row" key={routine.id}>
                <div>
                  <strong>{routine.name}</strong>
                  <small>{cadenceLabel(routine.interval_minutes)}</small>
                  {routine.last_run_unix_ms && (
                    <small>
                      last run:{" "}
                      {new Date(routine.last_run_unix_ms).toLocaleString()}
                    </small>
                  )}
                  {routine.last_status && <small>{routine.last_status}</small>}
                  {routine.last_skip_reason && <small>Skipped: {routine.last_skip_reason}</small>}
                </div>
                <label>
                  <input
                    type="checkbox"
                    aria-label={routine.name}
                    checked={routine.enabled}
                    onChange={() => void toggle(routine)}
                  />{" "}
                  Enabled
                </label>
                <button className="pill" onClick={() => openEditor(routine)}>
                  Edit
                </button>
                <button
                  className="pill"
                  onClick={() => void remove(routine.id)}
                >
                  Delete
                </button>
              </div>
            ))}
          </div>
          {editing === false && (
            <button className="pill primary" onClick={() => openEditor()}>
              Create Routine
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
          <label className="ws-field">
            <span>Cadence</span>
            <select
              value={intervalMinutes}
              onChange={(event) =>
                setIntervalMinutes(Number(event.currentTarget.value))
              }
            >
              {CADENCES.map((cadence) => (
                <option key={cadence.minutes} value={cadence.minutes}>
                  {cadence.label}
                </option>
              ))}
            </select>
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
              disabled={!name.trim() || !instruction.trim()}
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
