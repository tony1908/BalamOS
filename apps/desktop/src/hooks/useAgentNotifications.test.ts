import { renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSessionState } from "../types";
import { useAgentNotifications } from "./useAgentNotifications";

vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn(),
  requestPermission: vi.fn(),
  sendNotification: vi.fn(),
}));

const options = {
  notify: vi.fn(),
  ensurePermission: () => Promise.resolve(true),
  isFocused: (): boolean => false,
};

describe("useAgentNotifications", () => {
  beforeEach(() => options.notify.mockClear());

  it("notifies when running becomes completed while unfocused", async () => {
    const { rerender } = renderHook(
      ({ state }) =>
        useAgentNotifications("bot-1", state, "Builder", true, options),
      { initialProps: { state: "running" as AgentSessionState } },
    );

    rerender({ state: "completed" as AgentSessionState });
    await vi.waitFor(() => expect(options.notify).toHaveBeenCalledOnce());
    expect(options.notify).toHaveBeenCalledWith({
      title: "Builder",
      body: "Finished responding",
    });
  });

  it.each([
    ["focused", { ...options, isFocused: (): boolean => true }],
    ["disabled", options],
  ])("does not notify when %s", async (_label, deps) => {
    const enabled = _label !== "disabled";
    const { rerender } = renderHook(
      ({ state }) =>
        useAgentNotifications("bot-1", state, "Builder", enabled, deps),
      { initialProps: { state: "running" as AgentSessionState } },
    );
    rerender({ state: "completed" as AgentSessionState });
    await Promise.resolve();
    expect(options.notify).not.toHaveBeenCalled();
  });

  it("does not notify for a non-running transition", async () => {
    const { rerender } = renderHook(
      ({ state }) =>
        useAgentNotifications("bot-1", state, "Builder", true, options),
      { initialProps: { state: "idle" as AgentSessionState } },
    );
    rerender({ state: "completed" as AgentSessionState });
    await Promise.resolve();
    expect(options.notify).not.toHaveBeenCalled();
  });

  it("does not notify across bot switches", async () => {
    const { rerender } = renderHook(
      ({ key, state }) =>
        useAgentNotifications(key, state, "Builder", true, options),
      { initialProps: { key: "bot-1", state: "running" as AgentSessionState } },
    );
    rerender({ key: "bot-2", state: "completed" as AgentSessionState });
    await Promise.resolve();
    expect(options.notify).not.toHaveBeenCalled();
  });
});
