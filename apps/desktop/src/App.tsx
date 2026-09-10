import { useEffect, useState } from "react";
import "./App.css";
import { useWorkspaces, type WorkspaceController } from "./hooks/useWorkspaces";
import { NewWorkspaceDialog } from "./components/NewWorkspaceDialog";
import { OrbitWorkspaceView } from "./components/OrbitWorkspaceView";
import { Avatar, botStatus } from "./components/workspacePresentation";
import type { AgentSessionState, Template } from "./types";
import { useAgentNotifications } from "./hooks/useAgentNotifications";
import { templateApi, applyTemplate } from "./api/templates";
import { botSettingsApi } from "./api/botSettings";
import { useBotSettings } from "./hooks/useBotSettings";
import { OnboardingCarousel } from "./components/OnboardingCarousel";
import { GovernancePanel } from "./components/GovernancePanel";
import { SecretsPanel } from "./components/SecretsPanel";
import { HederaPanel } from "./components/HederaPanel";
import { PluginsHub } from "./components/PluginsHub";

// BalamOS — local agent OS, styled after x.ai/bot: a Messages-style app where each
// workspace is an agent you chat with, plus a live OS pane you can reveal. The
// presentation is now backed entirely by the real workspace controller.

function runtimeLabel(controller: WorkspaceController): string {
  const { runtime, runtimeLoading } = controller;
  if (!runtime) return runtimeLoading ? "Connecting…" : "Daemon unavailable";
  if (!runtime.available) return runtime.message;
  if (!runtime.mutations_ready) return "Reconciling…";
  return "Daemon ready";
}

export default function App() {
  return <OrbitAppView controller={useWorkspaces()} />;
}

export function OrbitAppView({
  controller,
}: {
  controller: WorkspaceController;
}) {
  const [onboarded, setOnboarded] = useState(() => {
    try {
      return localStorage.getItem("balamos.onboarded") === "true";
    } catch {
      return false;
    }
  });
  const {
    workspaces,
    selectedId,
    select,
    create,
    error,
    runtime,
    runtimeLoading,
    mutationsReady,
    retry,
  } = controller;
  const [query, setQuery] = useState("");
  const [showOs, setShowOs] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [templates, setTemplates] = useState<Template[]>([]);
  const [governanceOpen, setGovernanceOpen] = useState(false);
  const [secretsOpen, setSecretsOpen] = useState(false);
  const [hederaOpen, setHederaOpen] = useState(false);
  const [pluginsOpen, setPluginsOpen] = useState(false);
  const [selectedAgentState, setSelectedAgentState] = useState<
    AgentSessionState | undefined
  >(undefined);
  const botSettings = useBotSettings();

  useEffect(() => {
    if (dialogOpen) {
      templateApi
        .list()
        .then(setTemplates)
        .catch(() => setTemplates([]));
    }
  }, [dialogOpen]);

  useEffect(() => setSelectedAgentState(undefined), [selectedId]);

  const selected = workspaces.find((row) => row.id === selectedId) ?? null;
  useAgentNotifications(
    selectedId ?? "",
    selectedAgentState,
    selected ? botSettings.get(selected.id).displayName || selected.name : "",
    selected ? botSettings.get(selected.id).notifications : false,
  );
  const needle = query.trim().toLowerCase();
  const list = workspaces.filter((row) =>
    `${row.name} ${row.host_path}`.toLowerCase().includes(needle),
  );
  const daemonAvailable = runtime?.available === true;
  const osVisible = showOs && selected?.state === "running";

  return (
    <div className="app">
      <div className={`body ${osVisible ? "with-os" : "no-os"}`}>
        <aside className="pane sb">
          <div className="sb-top">
            <span className="sb-brand">
              <span className="balamos-mark" aria-hidden="true" />
              BalamOS
            </span>
            <button
              className="sb-plus"
              aria-label="New agent"
              onClick={() => setDialogOpen(true)}
            >
              +
            </button>
          </div>

          <div className="search">
            <svg
              width="15"
              height="15"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              aria-hidden="true"
            >
              <circle cx="11" cy="11" r="7" />
              <path d="m21 21-4.3-4.3" />
            </svg>
            <input
              placeholder="Search"
              aria-label="Search workspaces"
              value={query}
              onChange={(event) => setQuery(event.currentTarget.value)}
            />
          </div>

          <nav className="agent-list" aria-label="Workspaces">
            {list.map((row) => (
              <button
                key={row.id}
                className={`agent ${row.id === selectedId ? "active" : ""}`}
                aria-current={row.id === selectedId ? "true" : undefined}
                title={row.host_path}
                onClick={() => select(row.id)}
              >
                <Avatar
                  workspace={row}
                  color={botSettings.get(row.id).avatarColor}
                />
                <span className="agent-main">
                  <span className="agent-name">
                    {botSettings.get(row.id).displayName || row.name}
                  </span>
                  <div className="agent-preview">
                    {botStatus(
                      row.state,
                      row.id === selectedId ? selectedAgentState : undefined,
                    )}
                  </div>
                </span>
              </button>
            ))}
            {list.length === 0 && (
              <p className="agent-empty">
                {workspaces.length === 0 ? "No workspaces yet." : "No matches."}
              </p>
            )}
          </nav>

          <div className="sb-foot" role="status" aria-label="Daemon status">
            <span
              className={`rt-dot ${
                daemonAvailable ? (mutationsReady ? "ok" : "warn") : "bad"
              }`}
            />
            <span className="rt-label">{runtimeLabel(controller)}</span>
            {runtime && !runtime.available && (
              <button className="rt-retry" onClick={() => void retry()}>
                Retry
              </button>
            )}
          </div>
          <div className="sb-governance">
            <button className="plugins-toggle" onClick={() => setSecretsOpen(true)}>Secrets</button>
            <button className="plugins-toggle" onClick={() => setPluginsOpen(true)}>Plugins</button>
            <button className="plugins-toggle" onClick={() => setGovernanceOpen(true)}>Governance</button>
          </div>
        </aside>

        {selected ? (
          <OrbitWorkspaceView
            workspace={selected}
            controller={controller}
            showOs={showOs}
            onToggleOs={() => setShowOs((value) => !value)}
            onAgentState={setSelectedAgentState}
            botSettings={{
              settings: botSettings.get(selectedId ?? ""),
              update: (patch) => botSettings.update(selectedId ?? "", patch),
            }}
          />
        ) : (
          <main className="pane conv">
            {error && (
              <div className="shell-error" role="alert">
                {error}
              </div>
            )}
            <div className="thread">
              <div className="sysline">
                {workspaces.length === 0
                  ? "Create a workspace to get started."
                  : "Select a workspace."}
              </div>
            </div>
          </main>
        )}
      </div>
      <GovernancePanel open={governanceOpen} onClose={() => setGovernanceOpen(false)} />
      <SecretsPanel open={secretsOpen} onClose={() => setSecretsOpen(false)} />
      <HederaPanel workspaceId={selectedId ?? "default"} open={hederaOpen} onClose={() => setHederaOpen(false)} />
      <PluginsHub open={pluginsOpen} onClose={() => setPluginsOpen(false)} workspaceId={selectedId ?? null} onOpenHedera={() => { setPluginsOpen(false); setHederaOpen(true); }} />

      <NewWorkspaceDialog
        open={dialogOpen}
        mutationsReady={mutationsReady}
        templates={templates}
        onCreate={async (
          name,
          hostPath,
          profile,
          harness,
          template,
          avatarColor,
          model,
          reasoningEffort,
        ) => {
          const row = await create(
            name,
            hostPath,
            profile,
            harness,
            model,
            reasoningEffort,
          );
          if (!row) return;
          if (template) await applyTemplate(row.id, template);
          if (avatarColor) {
            const base = template
              ? {
                  displayName: template.settings.display_name ?? undefined,
                  label: template.settings.label ?? undefined,
                  description: template.settings.description ?? undefined,
                  notifications: template.settings.notifications,
                }
              : { notifications: true };
            await botSettingsApi.set(row.id, { ...base, avatarColor });
          }
          if (template || avatarColor) botSettings.reload();
        }}
        onClose={() => setDialogOpen(false)}
      />
      {!onboarded && (
        <OnboardingCarousel
          onDone={() => {
            try {
              localStorage.setItem("balamos.onboarded", "true");
            } catch {
              // Continue without persistence if storage is unavailable.
            }
            setOnboarded(true);
            if (workspaces.length === 0) {
              setDialogOpen(true);
            }
          }}
        />
      )}
      {!runtime && runtimeLoading && (
        <div className="docker-overlay" role="status" aria-label="Connecting">
          <div className="docker-card">
            <span className="spinner spinner-lg" aria-hidden="true" />
            <p>Connecting to BalamOS…</p>
          </div>
        </div>
      )}
      {runtime && !runtime.available && (
        <div
          className="docker-overlay"
          role="alertdialog"
          aria-label="Docker not running"
        >
          <div className="docker-card">
            <div className="docker-icon" aria-hidden="true">
              🐳
            </div>
            <h2>We need Docker to work! 🐳</h2>
            <p>
              Your bots live inside Docker, and it's taking a nap. Wake up
              Docker Desktop, then hit Retry.
            </p>
            {runtime.message && (
              <p className="docker-detail">{runtime.message}</p>
            )}
            <button
              className="pill primary"
              onClick={() => void retry()}
              disabled={runtimeLoading}
            >
              {runtimeLoading ? (
                <>
                  <span className="spinner" aria-hidden="true" /> Reconnecting…
                </>
              ) : (
                "Retry"
              )}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
