import { useEffect, useRef } from "react";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import type { AgentSessionState } from "../types";

const RESTING: ReadonlySet<AgentSessionState> = new Set(["completed", "idle"]);

async function defaultEnsurePermission(): Promise<boolean> {
  let granted = await isPermissionGranted();
  if (!granted) granted = (await requestPermission()) === "granted";
  return granted;
}

export function useAgentNotifications(
  key: string,
  state: AgentSessionState | undefined,
  botName: string,
  enabled: boolean,
  deps: {
    notify?: (options: { title: string; body: string }) => void;
    ensurePermission?: () => Promise<boolean>;
    isFocused?: () => boolean;
  } = {},
) {
  const notify = deps.notify ?? sendNotification;
  const ensurePermission = deps.ensurePermission ?? defaultEnsurePermission;
  const isFocused =
    deps.isFocused ??
    (() => (typeof document !== "undefined" ? document.hasFocus() : true));
  const prev = useRef<{ key: string; state?: AgentSessionState }>({ key });

  useEffect(() => {
    const last = prev.current;
    const sameKey = last.key === key;
    prev.current = { key, state };
    if (!enabled || !sameKey) return;
    if (
      last.state === "running" &&
      state &&
      RESTING.has(state) &&
      !isFocused()
    ) {
      void ensurePermission().then((granted) => {
        if (granted) notify({ title: botName, body: "Finished responding" });
      });
    }
  }, [key, state, enabled, botName, notify, ensurePermission, isFocused]);
}
