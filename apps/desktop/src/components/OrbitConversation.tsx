import { useEffect, useMemo, useRef, useState } from "react";
import type React from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import ReactMarkdown from "react-markdown";
import {
  useAgentSession,
  type AgentController,
} from "../hooks/useAgentSession";
import { buildAgentFeed, type AgentFeedItem } from "./agentFeed";
import type {
  AgentSessionState,
  ApprovalDecision,
  AgentPlanStep,
  Workspace,
} from "../types";

const PLAN_STEP_CLASS: Record<AgentPlanStep["state"], string> = {
  completed: "done",
  in_progress: "active",
  pending: "todo",
};

const DECISION_LABEL: Record<ApprovalDecision, string> = {
  accept: "Approved once",
  accept_for_session: "Approved for session",
  decline: "Declined",
  cancel: "Canceled",
};

// Real controller wrapper: the visual view stays injectable for tests.
export function OrbitConversation({
  workspace,
  onAgentState,
}: {
  workspace: Workspace;
  onAgentState?: (state: AgentSessionState | undefined) => void;
}) {
  const agent = useAgentSession(workspace);
  return (
    <OrbitConversationView
      agent={agent}
      workspaceName={workspace.name}
      onAgentState={onAgentState}
    />
  );
}

export function OrbitConversationView({
  agent,
  workspaceName,
  openSignIn = (url) => void openUrl(url),
  onAgentState,
}: {
  agent: AgentController;
  workspaceName: string;
  openSignIn?: (url: string) => void;
  onAgentState?: (state: AgentSessionState | undefined) => void;
}) {
  const state = agent.session?.state;
  useEffect(() => {
    onAgentState?.(state);
  }, [state, onAgentState]);
  const canPrompt =
    state === "idle" || state === "completed" || state === "interrupted";
  const active = state === "running" || state === "waiting_for_approval";
  const feed = useMemo(
    () => buildAgentFeed(agent.events, state ?? "idle"),
    [agent.events, state],
  );
  const logRef = useRef<HTMLDivElement>(null);
  const pinnedRef = useRef(true);
  useEffect(() => {
    const el = logRef.current;
    if (el && pinnedRef.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [feed]);

  return (
    <>
      <div
        className="thread"
        role="log"
        aria-label="Agent activity"
        aria-live="polite"
        ref={logRef}
        onScroll={(e) => {
          const el = e.currentTarget;
          pinnedRef.current =
            el.scrollHeight - el.scrollTop - el.clientHeight < 80;
        }}
      >
        {agent.loading && !agent.session && (
          <div className="sysline">Starting agent…</div>
        )}
        {feed.map((item) => (
          <FeedRow
            key={item.key}
            item={item}
            resolve={agent.resolve}
            openSignIn={openSignIn}
          />
        ))}
        {state === "running" && (
          <div className="row in" aria-live="polite">
            <div className="bubble in working" aria-label="Agent is working">
              <span className="working-dots" aria-hidden="true">
                <i />
                <i />
                <i />
              </span>
            </div>
          </div>
        )}
        {agent.error && (
          <div className="row in">
            <div className="bubble in error" role="alert">
              {agent.error}{" "}
              <button
                className="a-btn reject"
                onClick={() => void agent.retry()}
              >
                Retry
              </button>
            </div>
          </div>
        )}
      </div>
      <Composer
        canPrompt={canPrompt}
        active={active}
        needsAuth={state === "needs_authentication"}
        workspaceName={workspaceName}
        prompt={agent.prompt}
        interrupt={agent.interrupt}
      />
    </>
  );
}

function FeedRow({
  item,
  resolve,
  openSignIn,
}: {
  item: AgentFeedItem;
  resolve: (id: string, decision: ApprovalDecision) => Promise<void>;
  openSignIn: (url: string) => void;
}) {
  if (item.type === "message")
    return (
      <div className="row in">
        <div className="bubble in markdown">
          <ReactMarkdown
            components={{
              a: ({ href, children }) => (
                <a
                  href={href}
                  onClick={(event) => {
                    event.preventDefault();
                    if (href) openSignIn(href);
                  }}
                >
                  {children}
                </a>
              ),
            }}
          >
            {item.text}
          </ReactMarkdown>
        </div>
      </div>
    );

  if (item.type === "user")
    return (
      <div className="row out">
        <div className="bubble out">{item.text}</div>
      </div>
    );

  if (item.type === "plan") {
    const done = item.steps.filter((step) => step.state === "completed").length;
    return (
      <div className="row in">
        <div className="bubble in plan">
          <div className="plan-title">
            Plan
            <span className="k">
              {done}/{item.steps.length}
            </span>
          </div>
          {item.explanation && (
            <div className="plan-note">{item.explanation}</div>
          )}
          <ul className="plan-steps">
            {item.steps.map((step, index) => (
              <li
                key={`${index}-${step.text}`}
                className={PLAN_STEP_CLASS[step.state]}
              >
                <span className="box" aria-hidden="true" />
                {step.text}
                <span className="sr-only">
                  {" "}
                  ({step.state.replace("_", " ")})
                </span>
              </li>
            ))}
          </ul>
        </div>
      </div>
    );
  }

  if (item.type === "auth")
    return (
      <div className="row in">
        <div className="bubble in approval">
          <div className="a-head">Sign in to Codex</div>
          <div className="a-reason">
            Authorize Codex to continue in this workspace.
          </div>
          {item.userCode && <code>{item.userCode}</code>}
          <div className="a-actions">
            <button
              className="a-btn approve"
              onClick={() => openSignIn(item.url)}
            >
              Open sign-in
            </button>
          </div>
        </div>
      </div>
    );

  if (item.type === "error")
    return (
      <div className="row in">
        <div className="bubble in error" role="alert">
          {item.message}
        </div>
      </div>
    );

  if (item.type === "usage") return null;

  // approval
  return (
    <div className="row in">
      <div className="bubble in approval">
        <div className="a-head">⚠ Approval requested</div>
        <div className="a-reason">{item.title}</div>
        {item.detail && <code>{item.detail}</code>}
        {item.status === "pending" ? (
          <div className="a-actions">
            <button
              className="a-btn approve"
              onClick={() => void resolve(item.requestId, "accept")}
            >
              Approve once
            </button>
            <button
              className="a-btn"
              onClick={() => void resolve(item.requestId, "accept_for_session")}
            >
              Approve for session
            </button>
            <button
              className="a-btn reject"
              onClick={() => void resolve(item.requestId, "decline")}
            >
              Decline
            </button>
          </div>
        ) : (
          <div className="a-resolved">
            {item.decision ? DECISION_LABEL[item.decision] : "Resolved"}
          </div>
        )}
      </div>
    </div>
  );
}

function Composer({
  canPrompt,
  active,
  needsAuth,
  workspaceName,
  prompt,
  interrupt,
}: {
  canPrompt: boolean;
  active: boolean;
  needsAuth: boolean;
  workspaceName: string;
  prompt: (value: string) => Promise<void>;
  interrupt: () => Promise<void>;
}) {
  const [draft, setDraft] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const draftRef = useRef(draft);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    const submittedDraft = draft;
    const value = submittedDraft.trim();
    if (!value || !canPrompt || submitting) return;
    setSubmitting(true);
    try {
      await prompt(value);
      if (draftRef.current === submittedDraft) {
        draftRef.current = "";
        setDraft("");
        if (textareaRef.current) textareaRef.current.style.height = "";
      }
    } catch {
      // Preserve the draft so the user can retry after a failed prompt.
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="composer">
      {active && (
        <div className="composer-status">
          <button
            type="button"
            className="pill danger"
            onClick={() => void interrupt()}
          >
            Interrupt
          </button>
        </div>
      )}
      <form className="composer-pill" onSubmit={submit}>
        <textarea
          ref={textareaRef}
          rows={1}
          aria-label={`Message ${workspaceName}`}
          placeholder={
            needsAuth ? "Complete sign-in first" : `Message ${workspaceName}`
          }
          value={draft}
          disabled={!canPrompt}
          onChange={(event) => {
            const textarea = event.currentTarget;
            textarea.style.height = "auto";
            textarea.style.height = `${Math.min(textarea.scrollHeight, 132)}px`;
            draftRef.current = textarea.value;
            setDraft(textarea.value);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              event.currentTarget.form?.requestSubmit();
            }
          }}
        />
        <button
          type="submit"
          className="c-send"
          aria-label="Send message"
          disabled={!canPrompt || !draft.trim() || submitting}
        >
          ↑
        </button>
      </form>
    </div>
  );
}
