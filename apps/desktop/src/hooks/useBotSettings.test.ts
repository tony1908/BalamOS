import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api/botSettings", () => ({
  botSettingsApi: { list: vi.fn(), set: vi.fn() },
}));

import { botSettingsApi } from "../api/botSettings";
import { useBotSettings } from "./useBotSettings";

describe("useBotSettings", () => {
  beforeEach(() => {
    vi.mocked(botSettingsApi.list).mockReset();
    vi.mocked(botSettingsApi.set)
      .mockReset()
      .mockImplementation((_, settings) => Promise.resolve(settings));
  });

  it("loads settings from the daemon", async () => {
    vi.mocked(botSettingsApi.list).mockResolvedValue({
      "ws-a": { displayName: "Alpha", notifications: true },
    });

    const { result } = renderHook(() => useBotSettings());

    await waitFor(() =>
      expect(result.current.get("ws-a")).toEqual({
        displayName: "Alpha",
        notifications: true,
      }),
    );
    expect(result.current.get("unknown")).toEqual({ notifications: true });
  });

  it("optimistically updates and persists merged settings", async () => {
    vi.mocked(botSettingsApi.list).mockResolvedValue({
      "ws-a": { displayName: "Alpha", notifications: true },
    });
    const { result } = renderHook(() => useBotSettings());
    await waitFor(() =>
      expect(result.current.get("ws-a").displayName).toBe("Alpha"),
    );

    act(() => result.current.update("ws-a", { label: "Primary" }));

    expect(botSettingsApi.set).toHaveBeenCalledWith("ws-a", {
      displayName: "Alpha",
      label: "Primary",
      notifications: true,
    });
    expect(result.current.get("ws-a")).toEqual({
      displayName: "Alpha",
      label: "Primary",
      notifications: true,
    });
  });

  it("uses defaults when the daemon returns nothing", async () => {
    vi.mocked(botSettingsApi.list).mockResolvedValue({});

    const { result } = renderHook(() => useBotSettings());

    await waitFor(() =>
      expect(result.current.get("ws-a")).toEqual({ notifications: true }),
    );
  });

  it("reloads settings from the daemon", async () => {
    vi.mocked(botSettingsApi.list).mockResolvedValue({
      "ws-a": { displayName: "Alpha", notifications: true },
    });
    const { result } = renderHook(() => useBotSettings());
    await waitFor(() =>
      expect(result.current.get("ws-a").displayName).toBe("Alpha"),
    );

    vi.mocked(botSettingsApi.list).mockResolvedValue({
      "ws-a": { displayName: "Beta", notifications: false },
    });
    await act(async () => {
      await result.current.reload();
    });

    await waitFor(() =>
      expect(result.current.get("ws-a")).toEqual({
        displayName: "Beta",
        notifications: false,
      }),
    );
  });
});
