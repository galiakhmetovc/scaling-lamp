import type { DelegateView, PendingApprovalView, ShellCommandView, UIEvent } from "../lib/types";
import { reverseToolLog } from "./model";

type ToolLogEntry = NonNullable<UIEvent["tool"]>;

type ToolsPaneProps = {
  approvals: PendingApprovalView[];
  commands: ShellCommandView[];
  toolLog: ToolLogEntry[];
  delegates: DelegateView[];
  onApprove: (approvalID: string) => void;
  onDeny: (approvalID: string) => void;
  onKill: (commandID: string) => void;
};

export function ToolsPane(props: ToolsPaneProps) {
  const { approvals, commands, toolLog, delegates, onApprove, onDeny, onKill } = props;
  const orderedToolLog = reverseToolLog(toolLog);

  return (
    <div className="three-stack">
      <section className="surface surface-primary">
        <div className="section-title">
          <span>Pending approvals</span>
          <span className="muted">{approvals.length}</span>
        </div>
        {approvals.length === 0 ? <p className="muted">No pending approvals.</p> : approvals.map((approval) => (
          <article key={approval.approval_id} className="detail-card">
            <div className="detail-main">
              <strong>{approval.command} {(approval.args ?? []).join(" ")}</strong>
              <div className="muted">{approval.message}</div>
              <div className="muted">{approval.cwd}</div>
            </div>
            <div className="action-row">
              <button onClick={() => onApprove(approval.approval_id)}>Approve</button>
              <button className="secondary" onClick={() => onDeny(approval.approval_id)}>Deny</button>
            </div>
          </article>
        ))}
      </section>
      <section className="surface surface-secondary">
        <div className="section-title">
          <span>Running commands</span>
          <span className="muted">{commands.length}</span>
        </div>
        {commands.length === 0 ? <p className="muted">No running shell commands.</p> : commands.map((command) => (
          <article key={command.command_id} className="detail-card">
            <div className="detail-main">
              <strong>{command.command} {(command.args ?? []).join(" ")}</strong>
              <div className="muted">{command.status}</div>
              {command.cwd ? <div className="muted">{command.cwd}</div> : null}
              {command.last_chunk ? <pre>{command.last_chunk}</pre> : null}
            </div>
            <button className="secondary" onClick={() => onKill(command.command_id)}>Kill</button>
          </article>
        ))}
      </section>
      <section className="surface surface-secondary">
        <div className="section-title">
          <span>Delegates</span>
          <span className="muted">{delegates.length}</span>
        </div>
        {delegates.length === 0 ? <p className="muted">No delegates.</p> : (
          <div className="delegate-list">
            {delegates.map((delegate) => (
              <article key={delegate.delegate_id} className="detail-card compact">
                <div className="detail-main">
                  <strong>{delegate.delegate_id}</strong>
                  <div className="muted">{delegate.status}</div>
                  {delegate.task ? <div className="muted">{delegate.task}</div> : null}
                </div>
              </article>
            ))}
          </div>
        )}
      </section>
      <section className="surface surface-secondary">
        <div className="section-title">
          <span>Tool log</span>
          <span className="muted">{orderedToolLog.length}</span>
        </div>
        <div className="tool-log">
          {orderedToolLog.length === 0 ? <p className="muted">No tool activity yet.</p> : orderedToolLog.map((item, index) => (
            <article key={`${item.name}-${index}`} className="tool-log-item">
              <strong>{item.name}</strong>
              <span>{item.phase}</span>
              {item.result_text ? <pre>{item.result_text}</pre> : null}
              {item.error_text ? <div className="error">{item.error_text}</div> : null}
            </article>
          ))}
        </div>
      </section>
    </div>
  );
}
