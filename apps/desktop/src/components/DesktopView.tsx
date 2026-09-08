import { useCallback, useEffect, useRef, useState } from "react";
import type { DesktopSession } from "../types";
import { workspaceApi } from "../api/workspaces";

type ViewState =
  "frame_loading" | "live" | "refreshing" | "degraded" | "expired";
type Props = {
  workspaceId: string;
  createSession?: (id: string) => Promise<DesktopSession>;
};

export default function DesktopView({
  workspaceId,
  createSession = workspaceApi.createDesktopSession,
}: Props) {
  const [displayed, setDisplayed] = useState<DesktopSession | null>(null);
  const [state, setState] = useState<ViewState>("frame_loading");
  const [connecting, setConnecting] = useState(true);
  const generation = useRef(0);
  const displayedRef = useRef<DesktopSession | null>(null);
  const lastRecovery = useRef(0);
  const refresh = useCallback(async () => {
    const request = ++generation.current;
    setState((current) => (displayed ? "refreshing" : current));
    try {
      const next = await createSession(workspaceId);
      if (request !== generation.current) return;
      displayedRef.current = next;
      setDisplayed(next);
      setState("frame_loading");
    } catch {
      if (request === generation.current) setState("degraded");
    }
  }, [createSession, displayed, workspaceId]);
  useEffect(() => {
    displayedRef.current = null;
    setDisplayed(null);
    void refresh();
    return () => {
      generation.current += 1;
    };
  }, [workspaceId, createSession]);
  useEffect(() => {
    const handler = (event: MessageEvent) => {
      const data = event.data;
      if (data?.source !== "orbit-desktop" || data.type !== "session-invalid") {
        return;
      }
      const now = Date.now();
      if (now - lastRecovery.current < 3_000) return;
      lastRecovery.current = now;
      void refresh();
    };
    window.addEventListener("message", handler);
    return () => window.removeEventListener("message", handler);
  }, [refresh]);
  useEffect(() => {
    setConnecting(true);
  }, [displayed?.url]);
  useEffect(() => {
    if (!displayed) return;
    const identity = displayed;
    const remaining = Math.max(0, identity.expires_at_unix_ms - Date.now());
    const lead = Math.min(30_000, Math.max(1_000, remaining * 0.1));
    const refreshTimer = window.setTimeout(
      () => void refresh(),
      Math.max(1_000, remaining - lead),
    );
    const expiryTimer = window.setTimeout(() => {
      if (displayedRef.current !== identity) return;
      displayedRef.current = null;
      setDisplayed(null);
      setState("expired");
    }, remaining);
    return () => {
      window.clearTimeout(refreshTimer);
      window.clearTimeout(expiryTimer);
    };
  }, [displayed, refresh]);
  if (state === "degraded" || state === "expired") {
    return (
      <div className="desktop-state" role="alert">
        {state === "degraded"
          ? "Desktop unavailable"
          : "Desktop session expired"}
        <button onClick={() => void refresh()}>Retry</button>
      </div>
    );
  }
  return (
    <div
      className="desktop-stage"
      data-testid="desktop-stage"
      data-state={state}
    >
      {state === "refreshing" && (
        <div className="desktop-refreshing">Refreshing desktop session…</div>
      )}
      {displayed ? (
        <>
          <iframe
            key={displayed.url}
            className="desktop-frame"
            src={displayed.url}
            title="Ubuntu desktop"
            allow="autoplay; clipboard-read; clipboard-write; fullscreen"
            referrerPolicy="no-referrer"
            onLoad={() => {
              setState("live");
              window.setTimeout(() => setConnecting(false), 2_500);
            }}
            onError={() => setState("degraded")}
          />
          {/* ponytail: client-side block; server-side upgrade is KasmVNC's
              view_only URL param in the daemon's desktop-session URL. */}
          <div className="desktop-guard" aria-hidden="true" />
          <span className="desktop-viewonly">View only</span>
          {connecting && (
            <div className="desktop-connecting" role="status">
              Connecting to the desktop…
            </div>
          )}
        </>
      ) : (
        <div className="desktop-state">Loading Ubuntu desktop…</div>
      )}
    </div>
  );
}
