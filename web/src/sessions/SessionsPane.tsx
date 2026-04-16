import type { BootstrapPayload, SessionSummary } from "../lib/types";
import { buildSessionList, sessionSelectionIntent } from "./model";

type SessionsPaneProps = {
  bootstrap: BootstrapPayload | null;
  sessions: SessionSummary[];
  selectedSessionID: string;
  onSelectSession: (sessionID: string) => void;
  onCreateSession: () => void;
  onRenameSession: (sessionID: string, title: string) => void;
};

export function SessionsPane(props: SessionsPaneProps) {
  const { bootstrap, sessions, selectedSessionID, onSelectSession, onCreateSession, onRenameSession } = props;
  const items = buildSessionList(sessions, selectedSessionID);

  return (
    <div className="sessions-layout">
      <section className="surface surface-primary session-catalog">
        <div className="section-title">
          <span>Session catalog</span>
          <button onClick={onCreateSession}>New session</button>
        </div>
        <div className="session-list">
          {items.map((item) => (
            <button
              key={item.id}
              className={`session-item ${item.active ? "active" : ""}`}
              onClick={() => onSelectSession(sessionSelectionIntent(item.id).sessionID)}
            >
              <div className="session-item-main">
                <strong>{item.title}</strong>
                <span>{item.meta}</span>
              </div>
              <div className="session-item-meta">{item.activityText}</div>
              <div className="session-item-actions">
                <button
                  className="secondary small"
                  onClick={(event) => {
                    event.stopPropagation();
                    const nextTitle = window.prompt("Rename session", item.title);
                    if (!nextTitle || nextTitle.trim() === "" || nextTitle.trim() === item.title) {
                      return;
                    }
                    onRenameSession(item.id, nextTitle.trim());
                  }}
                >
                  Rename
                </button>
              </div>
            </button>
          ))}
        </div>
      </section>

      <section className="surface surface-secondary control-plane-panel">
        <div className="section-title">
          <span>Control plane</span>
          <span className="muted">{bootstrap?.agent_id ?? "-"}</span>
        </div>
        <dl className="kv-grid">
          <div>
            <dt>Listen</dt>
            <dd>{bootstrap?.listen_addr ?? "-"}</dd>
          </div>
          <div>
            <dt>Assets</dt>
            <dd>{bootstrap?.assets.mode ?? "-"}</dd>
          </div>
          <div>
            <dt>Endpoint</dt>
            <dd>{bootstrap?.transport.endpoint_path ?? "-"}</dd>
          </div>
          <div>
            <dt>WebSocket</dt>
            <dd>{bootstrap?.transport.websocket_path ?? "-"}</dd>
          </div>
        </dl>
      </section>
    </div>
  );
}
