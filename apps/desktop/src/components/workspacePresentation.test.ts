import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { Avatar, botStatus, prettyPath } from "./workspacePresentation";
import type { Workspace } from "../types";
import { createElement } from "react";

const workspace = { id: "ws-a", state: "running" } as Workspace;

describe("botStatus", () => {
  it("uses coworker-style words for lifecycle state", () => {
    expect(botStatus("running")).toBe("active");
    expect(botStatus("stopped")).toBe("asleep");
    expect(botStatus("failed")).toBe("needs attention");
    expect(botStatus("starting")).toBe("waking…");
  });

  it("prefers the live agent state when provided", () => {
    expect(botStatus("running", "waiting_for_approval")).toBe("waiting on you");
    expect(botStatus("running", "running")).toBe("working…");
    expect(botStatus("running", "needs_authentication")).toBe("needs sign-in");
  });

  it("falls back to lifecycle when there is no agent state", () => {
    expect(botStatus("stopped", undefined)).toBe("asleep");
  });
});

describe("prettyPath", () => {
  it("collapses macOS temp/disposable mounts to the folder name", () => {
    expect(
      prettyPath(
        "/private/var/folders/b2/jsqr7_s14sbccfj37t1xwh7w0000gn/T/orbit-disposable-20260828-48486-zn2v3e",
      ),
    ).toBe("orbit-disposable-20260828-48486-zn2v3e");
  });

  it("collapses /tmp mounts to the folder name", () => {
    expect(prettyPath("/tmp/orbit-scratch/project")).toBe("project");
  });

  it("collapses the home directory to ~", () => {
    expect(prettyPath("/Users/antonio/projects/payments-api")).toBe(
      "~/projects/payments-api",
    );
    expect(prettyPath("/home/dev/code/site")).toBe("~/code/site");
  });

  it("leaves other absolute paths unchanged", () => {
    expect(prettyPath("/projects/payments-api")).toBe("/projects/payments-api");
  });
});

describe("Avatar", () => {
  it("honors an explicit color and hashes the workspace when omitted", () => {
    const { rerender } = render(
      createElement(Avatar, { workspace, color: "#123456" }),
    );
    expect(document.querySelector(".avatar")).toHaveStyle({
      background: "#123456",
    });

    rerender(createElement(Avatar, { workspace }));
    expect(
      (document.querySelector(".avatar") as HTMLElement).style.background,
    ).toBeTruthy();
  });
});
