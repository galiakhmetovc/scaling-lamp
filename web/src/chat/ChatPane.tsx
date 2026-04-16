import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { SessionSnapshot } from "../lib/types";
import { buildBtwRuns, buildChatStatus, type BtwRun } from "./model";

type ChatPaneProps = {
  session: SessionSnapshot | null;
  streaming: string;
  status: string;
  input: string;
  now: Date;
  btwRuns: BtwRun[];
  onInput: (value: string) => void;
  onSend: () => void;
  onQueue: () => void;
  onRecallDraft: (draftID: string) => void;
};

export function ChatPane(props: ChatPaneProps) {
  const { session, streaming, status, input, now, btwRuns, onInput, onSend, onQueue, onRecallDraft } = props;
  const activeBtwCount = btwRuns.filter((run) => run.active).length;
  const statusView = buildChatStatus({ session, input, now, activeBtwCount, uiStatus: status });
  const btwViews = buildBtwRuns(btwRuns);

  return (
    <div className="chat-workspace">
      <section className="surface surface-primary timeline-panel">
        <div className="section-title">
          <span>Chat</span>
          <span className="muted">{session?.session_id ?? "no session"}</span>
        </div>
        <div className="timeline">
          {(session?.timeline ?? []).map((item, index) => (
            <article key={`${item.kind}-${index}`} className={`timeline-item ${item.kind}`}>
              {item.role && <div className="item-role">{item.role}</div>}
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{item.content}</ReactMarkdown>
            </article>
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
      </section>

      <div className="chat-ops-column">
        <section className="surface surface-secondary composer-panel">
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

        <section className="surface surface-secondary queue-panel">
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
  );
}
