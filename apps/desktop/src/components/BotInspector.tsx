import { useEffect, useState } from "react";
import DesktopView from "./DesktopView";
import type { BotSettings } from "../hooks/useBotSettings";
import type {
  AppOption,
  DesktopSession,
  Harness,
  ModelOption,
  Workspace,
} from "../types";
import { RoutinesPanel } from "./RoutinesPanel";
import { SkillsPanel } from "./SkillsPanel";
import { templateApi } from "../api/templates";
import { routineApi } from "../api/routines";
import { skillApi } from "../api/skills";
import { agentApi } from "../api/agents";
import { workspaceApi } from "../api/workspaces";
import {
  CUSTOM_MODEL,
  ModelSelect,
  REASONING_EFFORTS,
} from "./workspacePresentation";

const HARNESS_LABEL: Record<Harness, string> = {
  codex: "Codex",
  opencode: "opencode",
  claude_code: "Claude Code",
  antigravity: "Antigravity",
};

export function BotInspector({
  workspace,
  botSettings,
  createDesktopSession,
  onWorkspaceUpdated,
}: {
  workspace: Workspace;
  botSettings: {
    settings: BotSettings;
    update: (patch: Partial<BotSettings>) => void;
  };
  createDesktopSession: (id: string) => Promise<DesktopSession>;
  onWorkspaceUpdated?: (workspace: Workspace) => void;
}) {
  const [view, setView] = useState<
    "screen" | "settings" | "routines" | "skills"
  >("screen");
  const [expanded, setExpanded] = useState(false);
  const [sharing, setSharing] = useState(false);
  const [templateName, setTemplateName] = useState("");
  const [shared, setShared] = useState(false);
  const [controlling, setControlling] = useState(false);
  const { settings, update } = botSettings;
  const screenName = settings.displayName || workspace.name;

  const [models, setModels] = useState<ModelOption[]>([]);
  const [modelChoice, setModelChoice] = useState(workspace.model ?? "");
  const [customModel, setCustomModel] = useState(workspace.model ?? "");
  const [reasoningEffort, setReasoningEffort] = useState(
    workspace.reasoning_effort ?? "",
  );
  const [savingModel, setSavingModel] = useState(false);
  const [modelSaved, setModelSaved] = useState(false);
  const [modelError, setModelError] = useState<string | null>(null);
  const [appCatalog, setAppCatalog] = useState<AppOption[]>([]);
  const [selectedApps, setSelectedApps] = useState<string[]>(
    workspace.apps ?? [],
  );
  const [savingApps, setSavingApps] = useState(false);
  const [appsSaved, setAppsSaved] = useState(false);
  const [appsError, setAppsError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    workspaceApi
      .listModels(workspace.harness)
      .then((fetched) => {
        if (!live) return;
        setModels(fetched);
        if (workspace.model && !fetched.some((o) => o.id === workspace.model)) {
          setModelChoice(CUSTOM_MODEL);
          setCustomModel(workspace.model);
        }
      })
      .catch(() => {
        if (live) setModels([]);
      });
    return () => {
      live = false;
    };
  }, [workspace.harness]);

  useEffect(() => {
    let live = true;
    workspaceApi
      .listApps()
      .then((fetched) => {
        if (live) setAppCatalog(fetched);
      })
      .catch(() => {
        if (live) setAppCatalog([]);
      });
    return () => {
      live = false;
    };
  }, []);

  async function saveModel() {
    const finalModel =
      modelChoice === CUSTOM_MODEL ? customModel.trim() : modelChoice;
    setSavingModel(true);
    setModelError(null);
    setModelSaved(false);
    try {
      const updated = await workspaceApi.setWorkspaceModel(
        workspace.id,
        finalModel,
        reasoningEffort,
      );
      onWorkspaceUpdated?.(updated);
      setModelSaved(true);
    } catch (cause) {
      setModelError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSavingModel(false);
    }
  }

  async function saveApps() {
    setSavingApps(true);
    setAppsError(null);
    setAppsSaved(false);
    try {
      const updated = await workspaceApi.setWorkspaceApps(
        workspace.id,
        selectedApps,
      );
      onWorkspaceUpdated?.(updated);
      setAppsSaved(true);
    } catch (cause) {
      setAppsError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSavingApps(false);
    }
  }

  async function saveTemplate() {
    const name = templateName.trim();
    if (!name) return;
    const [routines, skills] = await Promise.all([
      routineApi.list(workspace.id),
      skillApi.list(workspace.id),
    ]);
    await templateApi.save(
      name,
      {
        display_name: settings.displayName ?? null,
        label: settings.label ?? null,
        description: settings.description ?? null,
        notifications: settings.notifications,
      },
      routines.map((r) => ({
        name: r.name,
        instruction: r.instruction,
        interval_minutes: r.interval_minutes,
        enabled: r.enabled,
      })),
      skills.map((s) => ({
        name: s.name,
        instruction: s.instruction,
        enabled: s.enabled,
      })),
    );
    setSharing(false);
    setTemplateName("");
    setShared(true);
  }

  return (
    <div className="inspector" aria-label="Bot inspector">
      <div className="inspector-tabs">
        {(["Settings", "Screen", "Routines", "Skills"] as const).map((name) => (
          <button
            key={name}
            className={`tab ${view === name.toLowerCase() ? "active" : ""}`}
            aria-pressed={view === name.toLowerCase()}
            onClick={() =>
              setView(
                name.toLowerCase() as
                  "screen" | "settings" | "routines" | "skills",
              )
            }
          >
            {name}
          </button>
        ))}
      </div>
      <div className="inspector-placeholder" hidden={view !== "routines"}>
        <RoutinesPanel workspaceId={workspace.id} />
      </div>
      <div className="inspector-placeholder" hidden={view !== "skills"}>
        <SkillsPanel workspaceId={workspace.id} />
      </div>
      <div
        className={
          view === "settings" ? "inspector-settings" : "inspector-screen"
        }
        hidden={view !== "settings"}
      >
        <h2>Settings</h2>
        <p>Runs on: {HARNESS_LABEL[workspace.harness]}</p>
        <label>
          Model
          <ModelSelect
            value={modelChoice}
            customValue={customModel}
            models={models}
            onChange={setModelChoice}
            onCustomChange={setCustomModel}
          />
        </label>
        <label>
          Reasoning effort
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
        <button
          type="button"
          className="pill"
          disabled={savingModel}
          onClick={() => void saveModel()}
        >
          {savingModel ? "Saving…" : "Save model"}
        </button>
        <span className="ws-field-hint">
          Applies on the bot's next start or Reset.
        </span>
        {modelSaved && <p className="ws-form-note">Model saved ✓</p>}
        {modelError && (
          <p className="ws-form-error" role="alert">
            {modelError}
          </p>
        )}
        <h3>Apps</h3>
        {appCatalog.map((entry) => (
          <label key={entry.id}>
            <input
              type="checkbox"
              aria-label={entry.label}
              checked={selectedApps.includes(entry.id)}
              onChange={(event) => {
                const checked = event.currentTarget.checked;
                setSelectedApps((current) =>
                  checked
                    ? [...current, entry.id]
                    : current.filter((id) => id !== entry.id),
                );
              }}
            />{" "}
            {entry.label} <small>{entry.description}</small>
          </label>
        ))}
        <button
          type="button"
          className="pill"
          disabled={savingApps}
          onClick={() => void saveApps()}
        >
          {savingApps ? "Saving…" : "Save apps"}
        </button>
        <span className="ws-field-hint">
          Applies on the bot's next start or Reset.
        </span>
        {appsSaved && <p className="ws-form-note">Apps saved ✓</p>}
        {appsError && (
          <p className="ws-form-error" role="alert">
            {appsError}
          </p>
        )}
        <label>
          Name
          <input
            value={settings.displayName ?? ""}
            onChange={(event) =>
              update({ displayName: event.currentTarget.value || undefined })
            }
          />
        </label>
        <label>
          Label
          <input
            value={settings.label ?? ""}
            onChange={(event) => update({ label: event.currentTarget.value })}
          />
        </label>
        <label>
          Description
          <textarea
            value={settings.description ?? ""}
            onChange={(event) =>
              update({ description: event.currentTarget.value })
            }
          />
        </label>
        <label>
          <input
            type="checkbox"
            checked={settings.notifications}
            onChange={(event) =>
              update({ notifications: event.currentTarget.checked })
            }
          />{" "}
          Notifications
        </label>
        {!sharing ? (
          <button
            type="button"
            className="pill"
            onClick={() => {
              setShared(false);
              setTemplateName(screenName);
              setSharing(true);
            }}
          >
            Share as template
          </button>
        ) : (
          <div className="ws-field">
            <input
              aria-label="Template name"
              placeholder="Template name"
              value={templateName}
              onChange={(event) => setTemplateName(event.currentTarget.value)}
            />
            <div className="routine-actions">
              <button
                type="button"
                className="pill primary"
                disabled={!templateName.trim()}
                onClick={() => void saveTemplate()}
              >
                Save template
              </button>
              <button
                type="button"
                className="pill"
                onClick={() => setSharing(false)}
              >
                Cancel
              </button>
            </div>
          </div>
        )}
        {shared && <p className="ws-form-note">Saved as a template ✓</p>}
      </div>
      <div
        className={
          view === "screen" ? "inspector-screen" : "inspector-screen is-hidden"
        }
        hidden={view !== "screen"}
      >
        <div className="os-head">
          <h2>{screenName}'s screen</h2>
          <span className="live-dot">
            <i aria-hidden="true" /> Live desktop
          </span>
          {controlling ? (
            <button className="pill" onClick={() => setControlling(false)}>
              Hand back to agent
            </button>
          ) : (
            <button
              className="pill"
              onClick={() => {
                setControlling(true);
                void agentApi.interrupt(workspace.id).catch(() => {});
              }}
            >
              Take control
            </button>
          )}
        </div>
        <div
          className={`screen-stage${expanded ? " expanded" : ""}`}
          data-testid="screen-stage"
        >
          <button
            className="screen-expand"
            aria-label={expanded ? "Close desktop" : "Expand desktop"}
            onClick={() => setExpanded((current) => !current)}
          >
            {expanded ? "Close" : "Expand"}
          </button>
          <div className="os-body">
            <div className="os-frame">
              <DesktopView
                workspaceId={workspace.id}
                createSession={createDesktopSession}
                controllable={controlling}
              />
            </div>
          </div>
        </div>
        <p className="screen-hint">
          {controlling
            ? "You're driving the desktop — the agent is paused. Hand back when you're done."
            : "Read-only preview. Use Expand for a larger view."}
        </p>
      </div>
    </div>
  );
}
