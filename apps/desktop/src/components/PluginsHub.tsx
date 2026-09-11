import { useEffect, useState } from "react";
import { skillApi } from "../api/skills";
import { type SkillApiLike } from "../hooks/useSkills";
import { SKILL_PLUGINS, type SkillPlugin } from "../skills/catalog";
import { HEDERA_MAINNET_SKILL_NAME } from "../skills/hederaMainnet";

type PluginsHubProps = {
  open: boolean;
  onClose: () => void;
  workspaceId: string | null;
  onOpenHedera: () => void;
  api?: SkillApiLike;
};

export function PluginsHub({
  open,
  onClose,
  workspaceId,
  onOpenHedera,
  api = skillApi,
}: PluginsHubProps) {
  const [installedNames, setInstalledNames] = useState<string[]>([]);
  const [selected, setSelected] = useState<SkillPlugin | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) setSelected(null);
    if (!open || !workspaceId) {
      setInstalledNames([]);
      return;
    }
    setError(null);
    void api
      .list(workspaceId)
      .then((skills) => setInstalledNames(skills.map((skill) => skill.name)))
      .catch((cause) => {
        setInstalledNames([]);
        setError(cause instanceof Error ? cause.message : String(cause));
      });
  }, [api, open, workspaceId]);

  async function install(plugin: SkillPlugin) {
    if (!workspaceId || pending || installedNames.includes(plugin.name)) return;
    setPending(true);
    setError(null);
    try {
      await api.create(workspaceId, plugin.name, plugin.instruction, true);
      setInstalledNames((current) => [...current, plugin.name]);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setPending(false);
    }
  }

  if (!open) return null;

  const isInstalled = (name: string) => installedNames.includes(name);

  return (
    <div className="docker-overlay" role="dialog" aria-label="Plugins">
      <div className="docker-card plugins-hub-card">
        <button className="icon-btn" onClick={onClose} aria-label="Close plugins">×</button>
        {selected ? (
          <div className="plugin-detail">
            <button className="plugin-detail-back" onClick={() => setSelected(null)}>
              ← All plugins
            </button>
            <div className="plugin-detail-head">
              <span
                className="plugin-icon"
                aria-hidden="true"
                style={{ backgroundImage: `linear-gradient(140deg, ${selected.accent}, ${selected.accentEnd})` }}
              >
                {selected.icon}
              </span>
              <div>
                <h2>{selected.title}</h2>
                <p className="sub">{selected.tagline}</p>
              </div>
            </div>
            <p className="plugin-detail-desc">{selected.description}</p>
            {!workspaceId && <p role="status">Select a workspace to install plugins.</p>}
            <div className="plugin-detail-actions">
              {isInstalled(selected.name) ? (
                <span className="plugin-installed">Installed</span>
              ) : (
                <button
                  className="pill primary"
                  aria-label={selected.addLabel}
                  disabled={!workspaceId || pending}
                  onClick={() => void install(selected)}
                >
                  {pending ? "Installing..." : "Install"}
                </button>
              )}
              {selected.name === HEDERA_MAINNET_SKILL_NAME && (
                <button className="pill" aria-label="Open wallet & transfers" onClick={onOpenHedera}>
                  Open wallet & transfers
                </button>
              )}
            </div>
            {error && <p role="alert" className="shell-error">{error}</p>}
          </div>
        ) : (
          <div>
            <div className="plugins-hub-head">
              <p className="eyebrow">PLUGINS</p>
              <h2>Integrations</h2>
              <p>Pick a plugin to install and set it up in this workspace.</p>
            </div>
            {!workspaceId && <p role="status">Select a workspace to manage plugins.</p>}
            <div className="plugin-grid">
              {SKILL_PLUGINS.map((plugin) => (
                <button
                  className="plugin-tile"
                  key={plugin.name}
                  aria-label={plugin.title}
                  onClick={() => setSelected(plugin)}
                >
                  <span
                    className="plugin-icon"
                    aria-hidden="true"
                    style={{ backgroundImage: `linear-gradient(140deg, ${plugin.accent}, ${plugin.accentEnd})` }}
                  >
                    {plugin.icon}
                  </span>
                  <span className="plugin-title">{plugin.title}</span>
                  <span className={`plugin-state${isInstalled(plugin.name) ? "" : " avail"}`}>
                    {isInstalled(plugin.name) ? "Installed" : "Available"}
                  </span>
                </button>
              ))}
            </div>
            {error && <p role="alert" className="shell-error">{error}</p>}
          </div>
        )}
      </div>
    </div>
  );
}
