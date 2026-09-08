import { useEffect, useRef, useState } from "react";
import { BotInspector } from "./BotInspector";
import { OrbitConversation } from "./OrbitConversation";
import { Avatar, PROFILE_LABEL, STATE_TEXT } from "./workspacePresentation";
import { workspaceApi } from "../api/workspaces";
import type { WorkspaceController } from "../hooks/useWorkspaces";
import type { AgentSessionState, DesktopSession, Workspace } from "../types";
import type { BotSettings as StoredBotSettings } from "../hooks/useBotSettings";
import { GovernancePanel } from "./GovernancePanel";
import { SecretsPanel } from "./SecretsPanel";

export function OrbitWorkspaceView({
  workspace,
  controller,
  showOs,
  onToggleOs,
  createDesktopSession = workspaceApi.createDesktopSession,
  onAgentState,
  botSettings,
}: {
  workspace: Workspace;
  controller: WorkspaceController;
  showOs: boolean;
  onToggleOs: () => void;
  createDesktopSession?: (id: string) => Promise<DesktopSession>;
  onAgentState?: (state: AgentSessionState | undefined) => void;
  botSettings: {
    settings: StoredBotSettings;
    update: (patch: Partial<StoredBotSettings>) => void;
  };
}) {
  const [confirm, setConfirm] = useState<null | "reset" | "delete">(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const [revealed, setRevealed] = useState(showOs);
  const [governanceOpen, setGovernanceOpen] = useState(false);
  const [secretsOpen, setSecretsOpen] = useState(false);

  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node))
        setMenuOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenuOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [menuOpen]);

  useEffect(() => {
    if (showOs) setRevealed(true);
  }, [showOs]);

  const running = workspace.state === "running";
  const canStart =
    workspace.state === "stopped" ||
    workspace.state === "failed" ||
    workspace.state === "deleted";
  const busy = controller.pending[workspace.id] === true;
  const disabled = !controller.mutationsReady || busy;

  const run = (operation: Promise<void>) => {
    void operation.catch(() => {
      // The controller records the failure and refreshes authoritative state.
    });
  };

  return (
    <>
      <main className="pane conv">
        <div className="conv-head">
          <Avatar
            workspace={workspace}
            size={30}
            color={botSettings.settings.avatarColor}
          />
          <div className="conv-id">
            <span className="conv-title">
              {botSettings.settings.displayName || workspace.name}
            </span>
            <span className="conv-sub" title={workspace.host_path}>
              {STATE_TEXT[workspace.state]}
            </span>
          </div>
          <div className="conv-actions">
            <button className="pill" onClick={() => setGovernanceOpen(true)}>Governance</button>
            <button className="pill" onClick={() => setSecretsOpen(true)}>Secrets</button>
            <span className="pill">
              <span className="dot" />
              {PROFILE_LABEL[workspace.profile]}
            </span>
            {running && (
              <button
                className={`icon-btn ${showOs ? "on" : ""}`}
                aria-label={showOs ? "Hide live desktop" : "Show live desktop"}
                onClick={onToggleOs}
              >
                <svg
                  width="18"
                  height="18"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.8"
                  aria-hidden="true"
                >
                  <rect x="2.5" y="4" width="19" height="13" rx="2" />
                  <path d="M8.5 20.5h7M12 17v3.5" />
                </svg>
              </button>
            )}
            <div className="actions-more" ref={menuRef}>
              <button
                className={`icon-btn ${menuOpen ? "on" : ""}`}
                aria-label="Bot actions"
                aria-haspopup="menu"
                aria-expanded={menuOpen}
                onClick={() => setMenuOpen((open) => !open)}
              >
                <svg
                  width="18"
                  height="18"
                  viewBox="0 0 24 24"
                  aria-hidden="true"
                >
                  <circle cx="5" cy="12" r="1.7" fill="currentColor" />
                  <circle cx="12" cy="12" r="1.7" fill="currentColor" />
                  <circle cx="19" cy="12" r="1.7" fill="currentColor" />
                </svg>
              </button>
              {menuOpen && (
                <div className="actions-menu" role="menu">
                  {running && (
                    <button
                      type="button"
                      role="menuitem"
                      className="menu-item"
                      disabled={disabled}
                      onClick={() => {
                        setMenuOpen(false);
                        setConfirm("reset");
                      }}
                    >
                      Reset
                    </button>
                  )}
                  {running && (
                    <button
                      type="button"
                      role="menuitem"
                      className="menu-item"
                      disabled={disabled}
                      onClick={() => {
                        setMenuOpen(false);
                        run(controller.stop(workspace.id));
                      }}
                    >
                      Stop
                    </button>
                  )}
                  {canStart && (
                    <button
                      type="button"
                      role="menuitem"
                      className="menu-item"
                      disabled={disabled}
                      onClick={() => {
                        setMenuOpen(false);
                        run(controller.start(workspace.id));
                      }}
                    >
                      Start
                    </button>
                  )}
                  <button
                    type="button"
                    role="menuitem"
                    className="menu-item danger"
                    disabled={disabled}
                    onClick={() => {
                      setMenuOpen(false);
                      setConfirm("delete");
                    }}
                  >
                    Delete
                  </button>
                </div>
              )}
            </div>
          </div>
        </div>

        {confirm && (
          <div
            className="confirm-bar"
            role="group"
            aria-label={
              confirm === "reset" ? "Confirm reset" : "Confirm delete"
            }
          >
            <span className="confirm-text">
              {confirm === "reset"
                ? "Reset recreates the container. Installed packages are lost; the project folder is kept."
                : "Delete removes this workspace. The host project folder is kept."}
            </span>
            <button className="pill" onClick={() => setConfirm(null)}>
              Cancel
            </button>
            <button
              className="pill danger"
              onClick={() => {
                setConfirm(null);
                run(
                  confirm === "reset"
                    ? controller.reset(workspace.id)
                    : controller.remove(workspace.id),
                );
              }}
            >
              {confirm === "reset" ? "Confirm reset" : "Confirm delete"}
            </button>
          </div>
        )}

        {controller.error && (
          <div className="shell-error" role="alert">
            {controller.error}
          </div>
        )}

        {running ? (
          <OrbitConversation
            key={workspace.id}
            workspace={workspace}
            onAgentState={onAgentState}
          />
        ) : (
          <div className="thread">
            <div className="sysline">
              {workspace.state === "failed"
                ? "The agent session failed. Start this workspace to retry."
                : "Start this workspace to begin a session."}
            </div>
          </div>
        )}
      </main>
      <GovernancePanel open={governanceOpen} workspaceId={workspace.id} onClose={() => setGovernanceOpen(false)} />
      <SecretsPanel open={secretsOpen} workspaceId={workspace.id} onClose={() => setSecretsOpen(false)} />

      {running && revealed && (
        <section
          className={`pane os ${showOs ? "" : "os-hidden"}`}
          aria-label="Live desktop"
          aria-hidden={!showOs}
        >
          <BotInspector
            key={workspace.id}
            workspace={workspace}
            botSettings={botSettings}
            createDesktopSession={createDesktopSession}
            onWorkspaceUpdated={controller.applyUpdate}
          />
        </section>
      )}
    </>
  );
}
