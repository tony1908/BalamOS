import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { OrbitConversationView } from "./OrbitConversation";
import type { AgentController } from "../hooks/useAgentSession";
import type { AgentEvent, AgentSession, AgentSessionState } from "../types";

const session = (state: AgentSessionState): AgentSession => ({
  workspace_id: "ws-1",
  thread_id: null,
  state,
  next_cursor: 0,
});

function agentStub(overrides: Partial<AgentController> = {}): AgentController {
  return {
    session: session("idle"),
    events: [],
    error: null,
    loading: false,
    prompt: vi.fn().mockResolvedValue(undefined),
    resolve: vi.fn().mockResolvedValue(undefined),
    interrupt: vi.fn().mockResolvedValue(undefined),
    retry: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

function view(agent: AgentController) {
  return render(
    <OrbitConversationView agent={agent} workspaceName="payments-api" />,
  );
}

describe("OrbitConversationView feed", () => {
  it("renders user prompt events as outgoing bubbles", () => {
    const viewResult = view(
      agentStub({
        events: [
          {
            cursor: 1,
            kind: { type: "user_prompt", data: { text: "hello bot" } },
          },
        ],
      }),
    );

    const text = screen.getByText("hello bot");
    expect(text).toBeVisible();
    expect(text).toBe(viewResult.container.querySelector(".bubble.out"));
  });

  it("reports the current agent state to its parent", () => {
    const onAgentState = vi.fn();
    render(
      <OrbitConversationView
        agent={agentStub({ session: session("waiting_for_approval") })}
        workspaceName="payments-api"
        onAgentState={onAgentState}
      />,
    );
    expect(onAgentState).toHaveBeenCalledWith("waiting_for_approval");
  });

  it("renders normalized assistant, plan, approval, and error rows", () => {
    const events: AgentEvent[] = [
      {
        cursor: 1,
        kind: { type: "assistant_delta", data: { text: "Working on it." } },
      },
      {
        cursor: 2,
        kind: {
          type: "plan_updated",
          data: {
            explanation: "Verify the signature",
            steps: [
              { text: "Read stripe.rs", state: "completed" },
              { text: "Add HMAC check", state: "in_progress" },
            ],
          },
        },
      },
      {
        cursor: 3,
        kind: {
          type: "tool_started",
          data: { item_id: "t1", title: "cargo test", detail: "compiling" },
        },
      },
      {
        cursor: 4,
        kind: {
          type: "approval_requested",
          data: {
            request_id: "r1",
            kind: "command",
            title: "Install libssl",
            detail: "sudo apt-get install",
          },
        },
      },
      {
        cursor: 5,
        kind: {
          type: "error",
          data: { code: "x", message: "harness error", retryable: true },
        },
      },
    ];
    view(agentStub({ events, session: session("waiting_for_approval") }));

    const log = screen.getByRole("log", { name: "Agent activity" });
    expect(log).toBeVisible();
    expect(within(log).getByText("Working on it.")).toBeVisible();
    expect(within(log).getByText("Read stripe.rs")).toBeVisible();
    expect(within(log).getByText("Install libssl")).toBeVisible();
    expect(within(log).getByText("harness error")).toBeVisible();
    expect(screen.getByRole("button", { name: "Approve once" })).toBeEnabled();
  });

  it("renders markdown in assistant messages", () => {
    view(
      agentStub({
        events: [
          {
            cursor: 1,
            kind: {
              type: "assistant_delta",
              data: { text: "**bold thing** and a list:\n\n1. one\n2. two" },
            },
          },
        ],
      }),
    );

    expect(screen.getByText("bold thing").closest("strong")).not.toBeNull();
    expect(screen.getByText("one")).toBeVisible();
  });

  it("resolves an approval with the chosen decision", async () => {
    const user = userEvent.setup();
    const resolve = vi.fn().mockResolvedValue(undefined);
    const events: AgentEvent[] = [
      {
        cursor: 1,
        kind: {
          type: "approval_requested",
          data: {
            request_id: "req-9",
            kind: "command",
            title: "Enable network",
            detail: null,
          },
        },
      },
    ];
    view(
      agentStub({ events, session: session("waiting_for_approval"), resolve }),
    );

    await user.click(
      screen.getByRole("button", { name: "Approve for session" }),
    );
    expect(resolve).toHaveBeenCalledWith("req-9", "accept_for_session");
  });

  it("renders the device auth prompt with an enabled Open sign-in action", () => {
    const events: AgentEvent[] = [
      {
        cursor: 1,
        kind: {
          type: "authentication_required",
          data: { url: "https://auth.test/device", user_code: "WXYZ-1234" },
        },
      },
    ];
    view(agentStub({ events, session: session("needs_authentication") }));

    expect(screen.getByText("WXYZ-1234")).toBeVisible();
    expect(screen.getByRole("button", { name: "Open sign-in" })).toBeEnabled();
  });

  it("offers Interrupt while the agent is active", async () => {
    const user = userEvent.setup();
    const interrupt = vi.fn().mockResolvedValue(undefined);
    view(agentStub({ session: session("running"), interrupt }));

    await user.click(screen.getByRole("button", { name: "Interrupt" }));
    expect(interrupt).toHaveBeenCalledTimes(1);
  });

  it("shows that the agent is working while the session is running", () => {
    view(agentStub({ session: session("running") }));

    expect(screen.getByLabelText("Agent is working")).toBeInTheDocument();
  });

  it("does not show that the agent is working when the session is idle", () => {
    view(agentStub({ session: session("idle") }));

    expect(screen.queryByLabelText("Agent is working")).toBeNull();
  });

  it("opens the device sign-in url through the injected opener", async () => {
    const user = userEvent.setup();
    const openSignIn = vi.fn();
    const events: AgentEvent[] = [
      {
        cursor: 1,
        kind: {
          type: "authentication_required",
          data: { url: "https://auth.test/device", user_code: "CODE" },
        },
      },
    ];
    render(
      <OrbitConversationView
        agent={agentStub({ events, session: session("needs_authentication") })}
        workspaceName="payments-api"
        openSignIn={openSignIn}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Open sign-in" }));
    expect(openSignIn).toHaveBeenCalledWith("https://auth.test/device");
  });

  it("shows a Retry control when the controller reports an error", async () => {
    const user = userEvent.setup();
    const retry = vi.fn().mockResolvedValue(undefined);
    view(agentStub({ error: "codex crashed", retry }));

    expect(screen.getByText("codex crashed")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(retry).toHaveBeenCalledTimes(1);
  });
});

describe("OrbitConversationView composer", () => {
  const deferred = () => {
    let resolve!: () => void;
    let reject!: (reason?: unknown) => void;
    const promise = new Promise<void>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    return { promise, resolve, reject };
  };

  it("clears the draft and textarea height after a successful unchanged prompt", async () => {
    const user = userEvent.setup();
    const control = deferred();
    const prompt = vi.fn().mockReturnValue(control.promise);
    view(agentStub({ prompt }));

    const box = screen.getByLabelText(
      "Message payments-api",
    ) as HTMLTextAreaElement;
    await user.type(box, "run the tests");
    await user.click(screen.getByRole("button", { name: "Send message" }));
    expect(prompt).toHaveBeenCalledWith("run the tests");

    control.resolve();
    await waitFor(() => expect(box).toHaveValue(""));
    expect(box.style.height).toBe("");
  });

  it("preserves the draft after a rejected prompt", async () => {
    const user = userEvent.setup();
    const control = deferred();
    const prompt = vi.fn().mockReturnValue(control.promise);
    view(agentStub({ prompt }));

    const box = screen.getByLabelText("Message payments-api");
    await user.type(box, "keep me");
    await user.click(screen.getByRole("button", { name: "Send message" }));

    control.reject(new Error("prompt failed"));
    await waitFor(() => expect(prompt).toHaveBeenCalledTimes(1));
    expect(box).toHaveValue("keep me");
  });

  it("preserves text typed while a prompt is in flight", async () => {
    const user = userEvent.setup();
    const control = deferred();
    const prompt = vi.fn().mockReturnValue(control.promise);
    view(agentStub({ prompt }));

    const box = screen.getByLabelText("Message payments-api");
    await user.type(box, "first");
    await user.click(screen.getByRole("button", { name: "Send message" }));
    await user.type(box, " and second");

    control.resolve();
    await waitFor(() => expect(prompt).toHaveBeenCalledTimes(1));
    expect(box).toHaveValue("first and second");
  });

  it("trims whitespace only for the submitted payload", async () => {
    const user = userEvent.setup();
    const prompt = vi.fn().mockResolvedValue(undefined);
    view(agentStub({ prompt }));

    const box = screen.getByLabelText("Message payments-api");
    await user.type(box, "  spaced  ");
    await user.click(screen.getByRole("button", { name: "Send message" }));
    expect(prompt).toHaveBeenCalledWith("spaced");
  });

  it("blocks duplicate submissions while one is pending", async () => {
    const user = userEvent.setup();
    const control = deferred();
    const prompt = vi.fn().mockReturnValue(control.promise);
    view(agentStub({ prompt }));

    const box = screen.getByLabelText("Message payments-api");
    await user.type(box, "once");
    const send = screen.getByRole("button", { name: "Send message" });
    await user.click(send);
    await user.click(send);
    expect(prompt).toHaveBeenCalledTimes(1);
  });
});
