import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import DesktopView from "./DesktopView";
import type { DesktopSession } from "../types";

const session = (
  url: string,
  expires = Date.now() + 100_000,
): DesktopSession => ({ url, expires_at_unix_ms: expires });

describe("DesktopView", () => {
  afterEach(() => {
    document.body.innerHTML = "";
    vi.useRealTimers();
  });
  it("requests the workspace and renders a live frame with safe attributes", async () => {
    const create = vi.fn().mockResolvedValue(session("https://desktop.test/a"));
    render(<DesktopView workspaceId="one" createSession={create} />);
    await act(async () => {});
    const frame = screen.getByTitle("Ubuntu desktop");
    expect(create).toHaveBeenCalledWith("one");
    expect(frame).toHaveAttribute("src", "https://desktop.test/a");
    expect(frame).toHaveAttribute(
      "allow",
      "autoplay; clipboard-read; clipboard-write; fullscreen",
    );
    expect(frame).toHaveAttribute("referrerpolicy", "no-referrer");
    fireEvent.load(frame);
    expect(screen.getByTestId("desktop-stage")).toHaveAttribute(
      "data-state",
      "live",
    );
    expect(screen.getByText("View only")).toBeInTheDocument();
  });

  it("shows the connecting overlay until the stream grace period ends", async () => {
    vi.useFakeTimers();
    const create = vi.fn().mockResolvedValue(session("https://desktop.test/a"));
    render(<DesktopView workspaceId="one" createSession={create} />);
    await act(async () => {});

    fireEvent.load(screen.getByTitle("Ubuntu desktop"));
    expect(screen.getByText("Connecting to the desktop…")).toBeInTheDocument();

    await act(async () => {
      vi.advanceTimersByTime(2_500);
    });
    expect(
      screen.queryByText("Connecting to the desktop…"),
    ).not.toBeInTheDocument();
  });

  it("recreates the session when the gateway reports an invalid session", async () => {
    const create = vi
      .fn()
      .mockResolvedValueOnce(session("https://desktop.test/old"))
      .mockResolvedValueOnce(session("https://desktop.test/new"));
    render(<DesktopView workspaceId="one" createSession={create} />);
    await act(async () => {});
    expect(create).toHaveBeenCalledTimes(1);

    await act(async () => {
      window.dispatchEvent(
        new MessageEvent("message", {
          data: { source: "orbit-desktop", type: "session-invalid" },
        }),
      );
    });

    await waitFor(() => expect(create).toHaveBeenCalledTimes(2));
  });

  it("ignores a stale request after the workspace changes", async () => {
    const pending: Array<(value: DesktopSession) => void> = [];
    const create = vi.fn(
      (id: string) =>
        new Promise<DesktopSession>((resolve) =>
          pending.push((value) => resolve(session(`${id}-${value.url}`))),
        ),
    );
    const { rerender } = render(
      <DesktopView workspaceId="one" createSession={create} />,
    );
    rerender(<DesktopView workspaceId="two" createSession={create} />);
    await act(async () => {
      pending[0](session("old"));
      pending[1](session("new"));
    });
    expect(screen.getAllByTitle("Ubuntu desktop")).toHaveLength(1);
    expect(screen.getByTitle("Ubuntu desktop")).toHaveAttribute(
      "src",
      "two-new",
    );
  });

  it("refreshes at the exact lead time and keeps the old frame visible", async () => {
    vi.useFakeTimers();
    const create = vi
      .fn()
      .mockResolvedValueOnce(session("old", Date.now() + 10_000))
      .mockImplementationOnce(() => new Promise(() => {}));
    render(<DesktopView workspaceId="one" createSession={create} />);
    await act(async () => {});
    await act(async () => {
      vi.advanceTimersByTime(8_900);
    });
    expect(create).toHaveBeenCalledTimes(1);
    await act(async () => {
      vi.advanceTimersByTime(100);
    });
    expect(create).toHaveBeenCalledTimes(2);
    expect(screen.getByTitle("Ubuntu desktop")).toHaveAttribute("src", "old");
    vi.useRealTimers();
  });

  it("uses min/max lead rules for short and long sessions", async () => {
    vi.useFakeTimers();
    const create = vi
      .fn()
      .mockResolvedValueOnce(session("short", Date.now() + 1_500))
      .mockResolvedValueOnce(session("long", Date.now() + 100_000));
    render(<DesktopView workspaceId="one" createSession={create} />);
    await act(async () => {});
    await act(async () => {
      vi.advanceTimersByTime(900);
    });
    expect(create).toHaveBeenCalledTimes(1);
    await act(async () => {
      vi.advanceTimersByTime(100);
    });
    expect(create).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });

  it("clears an expired frame while a replacement request is pending", async () => {
    vi.useFakeTimers();
    const create = vi
      .fn()
      .mockResolvedValueOnce(session("old", Date.now() + 1_000))
      .mockImplementationOnce(() => new Promise(() => {}));
    render(<DesktopView workspaceId="one" createSession={create} />);
    await act(async () => {});
    await act(async () => {
      vi.advanceTimersByTime(1_000);
    });
    expect(screen.queryByTitle("Ubuntu desktop")).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Desktop session expired",
    );
    vi.useRealTimers();
  });

  it("shows a generic retry after failure without exposing details", async () => {
    const create = vi
      .fn()
      .mockRejectedValueOnce(new Error("secret-url"))
      .mockResolvedValueOnce(session("replacement"));
    render(<DesktopView workspaceId="one" createSession={create} />);
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Desktop unavailable",
      ),
    );
    expect(screen.queryByText("secret-url")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() =>
      expect(screen.getByTitle("Ubuntu desktop")).toHaveAttribute(
        "src",
        "replacement",
      ),
    );
    expect(create).toHaveBeenCalledTimes(2);
  });

  it("cleans timers and suppresses results after unmount", async () => {
    vi.useFakeTimers();
    let resolve!: (value: DesktopSession) => void;
    const create = vi.fn(
      () =>
        new Promise<DesktopSession>((r) => {
          resolve = r;
        }),
    );
    const { unmount } = render(
      <DesktopView workspaceId="one" createSession={create} />,
    );
    unmount();
    await act(async () => {
      resolve(session("late"));
      vi.runAllTimers();
    });
    expect(screen.queryByTitle("Ubuntu desktop")).not.toBeInTheDocument();
    expect(vi.getTimerCount()).toBe(0);
    vi.useRealTimers();
  });
});
