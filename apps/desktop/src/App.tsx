import { useEffect, useRef, useState } from "react";
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
  const [manageOpen, setManageOpen] = useState(false);
  const manageRef = useRef<HTMLDivElement>(null);
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

  useEffect(() => {
    if (!manageOpen) return;
    const onDown = (event: MouseEvent) => {
      if (manageRef.current && !manageRef.current.contains(event.target as Node)) setManageOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [manageOpen]);

  const selected = workspaces.find((row) => row.id === selectedId) ?? null;
  useAgentNotifications(
    selectedId ?? "",
    selectedAgentState,
    selected ? botSettings.get(selected.id).displayName || selected.name : "",
    selected ? botSettings.get(selected.id).notifications : false,
  );
  const prevWorkspace = useRef<{ id?: string; state?: string }>({});
  useEffect(() => {
    const id = selected?.id;
    const state = selected?.state;
    const prev = prevWorkspace.current;
    // Auto-reveal the live desktop when THIS workspace starts (non-running -> running).
    if (id && state === "running" && prev.id === id && prev.state !== "running") {
      setShowOs(true);
    }
    prevWorkspace.current = { id, state };
  }, [selected?.id, selected?.state]);
  const needle = query.trim().toLowerCase();
  const list = workspaces.filter((row) =>
    `${row.name} ${row.host_path}`.toLowerCase().includes(needle),
  );
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

          <div className="sb-manage" ref={manageRef}>
            {manageOpen && (
              <div className="sb-manage-menu" role="menu">
                <button role="menuitem" className="sb-manage-item" onClick={() => { setManageOpen(false); setSecretsOpen(true); }}>Secrets</button>
                <button role="menuitem" className="sb-manage-item" onClick={() => { setManageOpen(false); setPluginsOpen(true); }}>Plugins</button>
                <button role="menuitem" className="sb-manage-item" onClick={() => { setManageOpen(false); setGovernanceOpen(true); }}>Governance</button>
              </div>
            )}
            <button
              className={`sb-manage-btn ${manageOpen ? "on" : ""}`}
              aria-haspopup="menu"
              aria-expanded={manageOpen}
              onClick={() => setManageOpen((open) => !open)}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
                <circle cx="12" cy="12" r="3" />
                <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
              </svg>
              Manage
            </button>
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
