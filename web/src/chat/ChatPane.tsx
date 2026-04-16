import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { SessionSnapshot, SettingsFieldState } from "../lib/types";
import { buildBtwRuns, buildChatStatus, buildCompactToolActivity, buildTimelineMarkdownBlock, type BtwRun } from "./model";

type ChatPaneProps = {
  session: SessionSnapshot | null;
  streaming: string;
  status: string;
  input: string;
  now: Date;
  btwRuns: BtwRun[];
  toolLog: Array<NonNullable<import("../lib/types").UIEvent["tool"]>>;
  onInput: (value: string) => void;
  onSend: () => void;
  onQueue: () => void;
  onRecallDraft: (draftID: string) => void;
  onLoadOlder: () => void;
  quickControls: SettingsFieldState[];
  settingsError: string;
  onQuickControlChange: (key: string, value: unknown) => void;
};

export function ChatPane(props: ChatPaneProps) {
  const { session, streaming, status, input, now, btwRuns, toolLog, onInput, onSend, onQueue, onRecallDraft, onLoadOlder, quickControls, settingsError, onQuickControlChange } = props;
  const activeBtwCount = btwRuns.filter((run) => run.active).length;
  const statusView = buildChatStatus({ session, input, now, activeBtwCount, uiStatus: status });
  const btwViews = buildBtwRuns(btwRuns);
  const liveTools = buildCompactToolActivity(toolLog);

  return (
    <div className="chat-workspace">
      <section className="surface surface-primary timeline-panel">
        <div className="section-title">
          <span>Chat</span>
          <span className="muted">{session?.session_id ?? "no session"}</span>
        </div>
        <div className="timeline-scroll">
          {session?.history.has_more && (
            <div className="history-controls">
              <button className="secondary" onClick={onLoadOlder}>
                {`Load older (${session.history.total_count - session.history.loaded_count} older)`}
              </button>
            </div>
          )}
          <div className="timeline">
          {liveTools.map((tool) => (
            <LiveToolItem key={tool.key} tool={tool} />
          ))}
          {(session?.timeline ?? []).map((item, index) => (
            <TimelineItem key={`${item.kind}-${index}`} kind={item.kind} role={item.role} content={item.content} />
          ))}
          {streaming && (
            <article className="timeline-item streaming">
              <div className="item-role">assistant</div>
              <pre>{streaming}</pre>
            </article>
          )}
          {btwViews.map((run) => (
            <article key={run.id} className="timeline-item btw">
              <div className="item-role">/btw</div>
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{`#### /btw\n**Q:** ${run.prompt}\n\n**Status:** ${run.statusText}\n\n${run.body}${run.providerMeta ? `\n\n\`${run.providerMeta}\`` : ""}`}</ReactMarkdown>
            </article>
          ))}
        </div>
        </div>
      </section>

      <div className="chat-ops-column surface surface-secondary">
        <div className="ops-scroll">
          <section className="quick-controls-panel">
            <div className="section-title">
              <span>Run controls</span>
              <span className="muted">{quickControls.length}</span>
            </div>
            {settingsError ? <div className="error-banner">{settingsError}</div> : null}
            <div className="quick-controls-grid">
              {quickControls.map((field) => (
                <label key={field.key}>
                  <span>{field.label}</span>
                  {field.type === "bool" ? (
                    <input
                      aria-label={field.key}
                      type="checkbox"
                      checked={Boolean(field.value)}
                      onChange={(event) => onQuickControlChange(field.key, event.target.checked)}
                    />
                  ) : field.type === "int" ? (
                    <input
                      aria-label={field.key}
                      type="number"
                      value={String(field.value ?? "")}
                      onChange={(event) => onQuickControlChange(field.key, event.target.value)}
                    />
                  ) : field.enum && field.enum.length > 0 ? (
                    <select aria-label={field.key} value={String(field.value ?? "")} onChange={(event) => onQuickControlChange(field.key, event.target.value)}>
                      {field.enum.map((option) => (
                        <option key={option} value={option}>
                          {option}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <input
                      aria-label={field.key}
                      value={String(field.value ?? "")}
                      onChange={(event) => onQuickControlChange(field.key, event.target.value)}
                    />
                  )}
                </label>
              ))}
            </div>
          </section>

          <section className="composer-panel">
            <textarea value={input} onChange={(event) => onInput(event.target.value)} placeholder="Send a message or /btw question" />
            <div className="composer-actions">
              <button onClick={onSend}>Send</button>
              <button className="secondary" onClick={onQueue}>
                Queue
              </button>
            </div>
            <div className="chat-statusbar">
              <span>{`provider ${statusView.provider}`}</span>
              <span>{`model ${statusView.model}`}</span>
              <span>{`run ${statusView.runText}`}</span>
              <span>{`ctx ~${statusView.contextTokens}`}</span>
              <span>{`queue ${statusView.queueCount}`}</span>
              <span>{`/btw ${statusView.activeBtwCount}`}</span>
              {statusView.lastUsageText && <span>{statusView.lastUsageText}</span>}
              <span>{statusView.statusText}</span>
            </div>
          </section>

          <section className="queue-panel">
            <div className="section-title">
              <span>Queued drafts</span>
              <span className="muted">{session?.queued_drafts.length ?? 0}</span>
            </div>
            <div className="queue-list">
              {(session?.queued_drafts ?? []).map((draft) => (
                <button key={draft.id} className="queue-item" onClick={() => onRecallDraft(draft.id)}>
                  <strong>{draft.text}</strong>
                  <span>{new Date(draft.queued_at).toLocaleTimeString()}</span>
                </button>
              ))}
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}

function LiveToolItem(props: { tool: ReturnType<typeof buildCompactToolActivity>[number] }) {
  const { tool } = props;
  if (tool.collapsible) {
    return (
      <article className={`timeline-item tool live-tool ${tool.state}`}>
        <details className="tool-result-toggle">
          <summary>{tool.summary}</summary>
          <pre className="tool-result-body-text">{tool.body}</pre>
        </details>
      </article>
    );
  }
  return (
    <article className={`timeline-item tool live-tool ${tool.state}`}>
      <div className="live-tool-summary">{tool.summary}</div>
    </article>
  );
}

function TimelineItem(props: { kind: "message" | "tool" | "plan"; role?: string; content: string }) {
  const { kind, role, content } = props;
  const block = buildTimelineMarkdownBlock(kind, content);
  if (kind === "tool" && block.collapsible) {
    return (
      <article className={`timeline-item ${kind}`}>
        <details className="tool-result-toggle">
          <summary>
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{block.summary}</ReactMarkdown>
          </summary>
          <div className="tool-result-body">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{block.body}</ReactMarkdown>
          </div>
        </details>
      </article>
    );
  }
  return (
    <article className={`timeline-item ${kind}`}>
      {role && <div className="item-role">{role}</div>}
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{content}</ReactMarkdown>
    </article>
  );
}
