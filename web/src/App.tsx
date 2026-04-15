import { useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { DaemonClient, loadRuntimeClientConfig } from "./lib/client";
import type {
  BootstrapPayload,
  PendingApprovalView,
  ProviderResultPayload,
  SessionSnapshot,
  SessionSummary,
  SettingsRawFileContent,
  SettingsSnapshot,
  ShellCommandView,
  UIEvent,
  WebsocketEnvelope,
} from "./lib/types";

type TabKey = "sessions" | "chat" | "plan" | "tools" | "settings";

type ToolLogEntry = NonNullable<UIEvent["tool"]>;

type BtwRun = {
  id: string;
  prompt: string;
  active: boolean;
  error?: string;
  result?: ProviderResultPayload;
};

type SessionUIState = {
  streaming: string;
  status: string;
  toolLog: ToolLogEntry[];
  btwRuns: BtwRun[];
  lastResult?: ProviderResultPayload;
};

export function App() {
  const clientRef = useRef<DaemonClient | null>(null);
  const [bootstrap, setBootstrap] = useState<BootstrapPayload | null>(null);
  const [connected, setConnected] = useState(false);
  const [activeTab, setActiveTab] = useState<TabKey>("chat");
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [selectedSessionID, setSelectedSessionID] = useState<string>("");
  const [sessionSnapshots, setSessionSnapshots] = useState<Record<string, SessionSnapshot>>({});
  const [sessionUI, setSessionUI] = useState<Record<string, SessionUIState>>({});
  const [chatInput, setChatInput] = useState("");
  const [planGoal, setPlanGoal] = useState("");
  const [planTask, setPlanTask] = useState("");
  const [planNote, setPlanNote] = useState("");
  const [settings, setSettings] = useState<SettingsSnapshot | null>(null);
  const [settingsDraft, setSettingsDraft] = useState<Record<string, unknown>>({});
  const [rawFile, setRawFile] = useState<SettingsRawFileContent | null>(null);
  const [selectedRawPath, setSelectedRawPath] = useState("");
  const [statusMessage, setStatusMessage] = useState("booting");
  const [errorMessage, setErrorMessage] = useState("");

  useEffect(() => {
    const abort = new AbortController();
    let localClient: DaemonClient | null = null;
    let unsubscribe = () => {};
    const load = async () => {
      try {
        const config = await loadRuntimeClientConfig();
        localClient = new DaemonClient(config);
        clientRef.current = localClient;
        const boot = await localClient.bootstrap(abort.signal);
        setBootstrap(boot);
        setSessions(boot.sessions);
        setSettings(boot.settings);
        setSettingsDraft(fieldDraftFromSnapshot(boot.settings));
        if (boot.sessions.length > 0) {
          setSelectedSessionID((current) => current || boot.sessions[0].session_id);
        }
        await localClient.connect();
        setConnected(true);
        setStatusMessage("connected");
        unsubscribe = localClient.onEnvelope((envelope) => {
          handleEnvelope(envelope);
        });
      } catch (error) {
        setErrorMessage(String(error));
      }
    };
    void load();
    return () => {
      abort.abort();
      unsubscribe();
      clientRef.current = null;
      localClient?.disconnect();
    };
  }, []);

  useEffect(() => {
    if (!selectedSessionID) {
      return;
    }
    if (sessionSnapshots[selectedSessionID]) {
      return;
    }
    void refreshSession(selectedSessionID);
  }, [selectedSessionID, sessionSnapshots]);

  useEffect(() => {
    if (!settings || selectedRawPath === "") {
      return;
    }
    void loadRawFile(selectedRawPath);
  }, [selectedRawPath]);

  const selectedSession = selectedSessionID ? sessionSnapshots[selectedSessionID] : null;
  const selectedUI = selectedSessionID ? sessionUI[selectedSessionID] ?? emptySessionUIState() : emptySessionUIState();
  const approxTokens = useMemo(() => {
    if (!selectedSession) {
      return 0;
    }
    return selectedSession.transcript.reduce((sum, item) => sum + Math.ceil((item.content || "").length / 4), 0);
  }, [selectedSession]);

  function applySessionSnapshot(snapshot: SessionSnapshot) {
    setSessionSnapshots((current) => ({ ...current, [snapshot.session_id]: snapshot }));
    setSessions((current) => upsertSessionSummary(current, snapshot));
    setSelectedSessionID((current) => current || snapshot.session_id);
  }

  function applyUIEvent(event: UIEvent) {
    if (!event.session_id) {
      return;
    }
    setSessionUI((current) => {
      const next = { ...current };
      const state = { ...emptySessionUIState(), ...next[event.session_id] };
      switch (event.kind) {
        case "stream.text":
          state.streaming += event.text || "";
          break;
        case "tool.started":
        case "tool.completed":
          if (event.tool) {
            state.toolLog = [...state.toolLog, event.tool].slice(-200);
          }
          break;
        case "status.changed":
          state.status = event.status || state.status;
          break;
        case "run.completed":
          state.status = "done";
          state.streaming = "";
          break;
      }
      next[event.session_id] = state;
      return next;
    });
  }

  function handleEnvelope(envelope: WebsocketEnvelope) {
    if (envelope.type === "ui_event" && envelope.event) {
      applyUIEvent(envelope.event);
      return;
    }
    if (envelope.type === "settings_applied") {
      void refreshSettings();
      return;
    }
    const payload = typeof envelope.payload === "object" && envelope.payload !== null ? (envelope.payload as Record<string, unknown>) : null;
    const sessionID = typeof payload?.session_id === "string" ? payload.session_id : undefined;
    if (!sessionID) {
      return;
    }
    void refreshSession(sessionID);
  }

  async function refreshSession(sessionID: string) {
    const client = clientRef.current;
    if (!client) {
      return;
    }
    const result = await client.command("session.get", { session_id: sessionID });
    applySessionSnapshot(result.session);
  }

  async function refreshSettings() {
    const client = clientRef.current;
    if (!client) {
      return;
    }
    const result = await client.command("settings.get");
    setSettings(result.settings);
    setSettingsDraft(fieldDraftFromSnapshot(result.settings));
  }

  async function loadRawFile(path: string) {
    const client = clientRef.current;
    if (!client) {
      return;
    }
    const result = await client.command("settings.raw.get", { path });
    setRawFile(result.file);
  }

  async function handleCreateSession() {
    const client = clientRef.current;
    if (!client) {
      return;
    }
    const result = await client.command("session.create");
    applySessionSnapshot(result.session);
    setSelectedSessionID(result.session.session_id);
    setActiveTab("chat");
  }

  async function handleSendChat() {
    const client = clientRef.current;
    const prompt = chatInput.trim();
    if (!client || !selectedSessionID || !prompt) {
      return;
    }
    if (prompt.startsWith("/btw ")) {
      const btwPrompt = prompt.slice(5).trim();
      if (btwPrompt) {
        const runID = `btw-${Date.now()}`;
        setSessionUI((current) => ({
          ...current,
          [selectedSessionID]: {
            ...emptySessionUIState(),
            ...current[selectedSessionID],
            btwRuns: [...(current[selectedSessionID]?.btwRuns ?? []), { id: runID, prompt: btwPrompt, active: true }],
          },
        }));
        setChatInput("");
        try {
          const result = await client.command("chat.btw", { session_id: selectedSessionID, prompt: btwPrompt });
          setSessionUI((current) => ({
            ...current,
            [selectedSessionID]: {
              ...emptySessionUIState(),
              ...current[selectedSessionID],
              btwRuns: (current[selectedSessionID]?.btwRuns ?? []).map((run) =>
                run.id === runID ? { ...run, active: false, result: result.result } : run,
              ),
            },
          }));
        } catch (error) {
          setSessionUI((current) => ({
            ...current,
            [selectedSessionID]: {
              ...emptySessionUIState(),
              ...current[selectedSessionID],
              btwRuns: (current[selectedSessionID]?.btwRuns ?? []).map((run) =>
                run.id === runID ? { ...run, active: false, error: String(error) } : run,
              ),
            },
          }));
        }
      }
      return;
    }
    setChatInput("");
    try {
      const result = await client.command("chat.send", { session_id: selectedSessionID, prompt });
      applySessionSnapshot(result.session);
      if (result.result) {
        setSessionUI((current) => ({
          ...current,
          [selectedSessionID]: {
            ...emptySessionUIState(),
            ...current[selectedSessionID],
            streaming: "",
            status: "idle",
            lastResult: result.result,
          },
        }));
      }
      setStatusMessage(result.queued ? "queued" : "sent");
    } catch (error) {
      setErrorMessage(String(error));
    }
  }

  async function handleQueueDraft() {
    const client = clientRef.current;
    if (!client || !selectedSessionID || !chatInput.trim()) {
      return;
    }
    const result = await client.command("draft.enqueue", { session_id: selectedSessionID, text: chatInput.trim() });
    applySessionSnapshot(result.session);
    setChatInput("");
    setStatusMessage("draft queued");
  }

  async function handleRecallDraft(draftID: string) {
    const client = clientRef.current;
    if (!client || !selectedSessionID) {
      return;
    }
    const result = await client.command("draft.recall", { session_id: selectedSessionID, draft_id: draftID });
    applySessionSnapshot(result.session);
    setChatInput(result.draft.text);
  }

  async function handleCreatePlan() {
    const client = clientRef.current;
    if (!client || !selectedSessionID || !planGoal.trim()) {
      return;
    }
    const result = await client.command("plan.create", { session_id: selectedSessionID, goal: planGoal.trim() });
    applySessionSnapshot(result.session);
    setPlanGoal("");
  }

  async function handleAddTask() {
    const client = clientRef.current;
    if (!client || !selectedSessionID || !planTask.trim()) {
      return;
    }
    const result = await client.command("plan.add_task", { session_id: selectedSessionID, description: planTask.trim() });
    applySessionSnapshot(result.session);
    setPlanTask("");
  }

  async function handleSetTaskStatus(taskID: string, status: string) {
    const client = clientRef.current;
    if (!client || !selectedSessionID) {
      return;
    }
    const result = await client.command("plan.set_task_status", { session_id: selectedSessionID, task_id: taskID, status });
    applySessionSnapshot(result.session);
  }

  async function handleAddTaskNote(taskID: string) {
    const client = clientRef.current;
    if (!client || !selectedSessionID || !planNote.trim()) {
      return;
    }
    const result = await client.command("plan.add_task_note", { session_id: selectedSessionID, task_id: taskID, note: planNote.trim() });
    applySessionSnapshot(result.session);
    setPlanNote("");
  }

  async function handleApproveShell(approvalID: string) {
    const client = clientRef.current;
    if (!client) {
      return;
    }
    const result = await client.command("shell.approve", { approval_id: approvalID });
    applySessionSnapshot(result.session);
  }

  async function handleDenyShell(approvalID: string) {
    const client = clientRef.current;
    if (!client) {
      return;
    }
    const result = await client.command("shell.deny", { approval_id: approvalID });
    applySessionSnapshot(result.session);
  }

  async function handleKillShell(commandID: string) {
    const client = clientRef.current;
    if (!client) {
      return;
    }
    const result = await client.command("shell.kill", { command_id: commandID });
    applySessionSnapshot(result.session);
  }

  async function handleApplySettings() {
    const client = clientRef.current;
    if (!client || !settings) {
      return;
    }
    const result = await client.command("settings.form.apply", {
      base_revision: settings.revision,
      values: settingsDraft,
    });
    setSettings(result.settings);
    setSettingsDraft(fieldDraftFromSnapshot(result.settings));
    setStatusMessage("settings applied");
  }

  async function handleApplyRaw() {
    const client = clientRef.current;
    if (!client || !rawFile) {
      return;
    }
    const result = await client.command("settings.raw.apply", {
      path: rawFile.path,
      base_revision: rawFile.revision,
      content: rawFile.content,
    });
    setSettings(result.settings);
    setSettingsDraft(fieldDraftFromSnapshot(result.settings));
    await loadRawFile(rawFile.path);
    setStatusMessage("raw config applied");
  }

  return (
    <div className="app-shell">
      <header className="topbar">
        <div>
          <div className="eyebrow">teamD operator surface</div>
          <h1>Daemon Console</h1>
        </div>
        <div className="topbar-meta">
          <span className={`signal ${connected ? "ok" : "down"}`}>{connected ? "websocket up" : "websocket down"}</span>
          <span>{bootstrap?.listen_addr ?? "listen pending"}</span>
          <span>{bootstrap?.agent_id ?? "agent loading"}</span>
        </div>
      </header>

      <nav className="tabs">
        {(["sessions", "chat", "plan", "tools", "settings"] as TabKey[]).map((tab) => (
          <button key={tab} className={tab === activeTab ? "active" : ""} onClick={() => setActiveTab(tab)}>
            {tab}
          </button>
        ))}
      </nav>

      <main className="workspace">
        <aside className="session-rail">
          <div className="section-title">
            <span>Sessions</span>
            <button onClick={() => void handleCreateSession()}>New</button>
          </div>
          <div className="session-list">
            {sessions.map((session) => (
              <button
                key={session.session_id}
                className={`session-item ${session.session_id === selectedSessionID ? "active" : ""}`}
                onClick={() => setSelectedSessionID(session.session_id)}
              >
                <strong>{session.session_id}</strong>
                <span>{session.message_count} messages</span>
              </button>
            ))}
          </div>
        </aside>

        <section className="main-panel">
          {activeTab === "sessions" && <SessionsView bootstrap={bootstrap} />}
          {activeTab === "chat" && (
            <ChatView
              session={selectedSession}
              ui={selectedUI}
              input={chatInput}
              approxTokens={approxTokens}
              onInput={setChatInput}
              onSend={() => void handleSendChat()}
              onQueue={() => void handleQueueDraft()}
              onRecallDraft={(id) => void handleRecallDraft(id)}
            />
          )}
          {activeTab === "plan" && (
            <PlanView
              session={selectedSession}
              goal={planGoal}
              task={planTask}
              note={planNote}
              onGoal={setPlanGoal}
              onTask={setPlanTask}
              onNote={setPlanNote}
              onCreatePlan={() => void handleCreatePlan()}
              onAddTask={() => void handleAddTask()}
              onSetTaskStatus={(taskID, status) => void handleSetTaskStatus(taskID, status)}
              onAddTaskNote={(taskID) => void handleAddTaskNote(taskID)}
            />
          )}
          {activeTab === "tools" && (
            <ToolsView
              approvals={selectedSession?.pending_approvals ?? []}
              commands={selectedSession?.running_commands ?? []}
              toolLog={selectedUI.toolLog}
              delegates={selectedSession?.delegates ?? []}
              onApprove={(id) => void handleApproveShell(id)}
              onDeny={(id) => void handleDenyShell(id)}
              onKill={(id) => void handleKillShell(id)}
            />
          )}
          {activeTab === "settings" && (
            <SettingsView
              settings={settings}
              draft={settingsDraft}
              rawFile={rawFile}
              selectedRawPath={selectedRawPath}
              onDraftChange={setSettingsDraft}
              onApply={() => void handleApplySettings()}
              onSelectRaw={(path) => setSelectedRawPath(path)}
              onRawChange={(content) => setRawFile((current) => (current ? { ...current, content } : current))}
              onApplyRaw={() => void handleApplyRaw()}
            />
          )}
        </section>
      </main>

      <footer className="statusline">
        <span>{statusMessage}</span>
        {selectedSession && <span>queue {selectedSession.queued_drafts.length}</span>}
        {selectedSession && <span>tokens ~{approxTokens}</span>}
        {selectedUI.lastResult && <span>{selectedUI.lastResult.provider}</span>}
        {selectedUI.lastResult && <span>{selectedUI.lastResult.model}</span>}
        {errorMessage && <span className="error">{errorMessage}</span>}
      </footer>
    </div>
  );
}

function SessionsView({ bootstrap }: { bootstrap: BootstrapPayload | null }) {
  return (
    <div className="panel-stack">
      <section className="panel">
        <h2>Control Plane</h2>
        <p className="muted">Shared daemon state for TUI, web, and future remote clients.</p>
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

function ChatView(props: {
  session: SessionSnapshot | null;
  ui: SessionUIState;
  input: string;
  approxTokens: number;
  onInput: (value: string) => void;
  onSend: () => void;
  onQueue: () => void;
  onRecallDraft: (draftID: string) => void;
}) {
  const { session, ui, input, approxTokens, onInput, onSend, onQueue, onRecallDraft } = props;
  return (
    <div className="chat-layout">
      <section className="panel timeline-panel">
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
          {ui.streaming && (
            <article className="timeline-item streaming">
              <div className="item-role">assistant</div>
              <pre>{ui.streaming}</pre>
            </article>
          )}
          {ui.btwRuns.map((run) => (
            <article key={run.id} className="timeline-item btw">
              <div className="item-role">/btw</div>
              <strong>{run.prompt}</strong>
              {run.active && <div className="muted">running</div>}
              {run.error && <div className="error">{run.error}</div>}
              {run.result && <ReactMarkdown remarkPlugins={[remarkGfm]}>{run.result.content}</ReactMarkdown>}
            </article>
          ))}
        </div>
      </section>

      <section className="panel composer-panel">
        <textarea value={input} onChange={(event) => onInput(event.target.value)} placeholder="Send a message or /btw question" />
        <div className="composer-actions">
          <button onClick={onSend}>Send</button>
          <button className="secondary" onClick={onQueue}>Queue</button>
        </div>
        <div className="status-chip-row">
          <span>{session?.main_run_active ? "main run active" : "idle"}</span>
          <span>{ui.status || "ready"}</span>
          <span>queue {session?.queued_drafts.length ?? 0}</span>
          <span>/btw {ui.btwRuns.filter((run) => run.active).length}</span>
          <span>tokens ~{approxTokens}</span>
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
  );
}

function PlanView(props: {
  session: SessionSnapshot | null;
  goal: string;
  task: string;
  note: string;
  onGoal: (value: string) => void;
  onTask: (value: string) => void;
  onNote: (value: string) => void;
  onCreatePlan: () => void;
  onAddTask: () => void;
  onSetTaskStatus: (taskID: string, status: string) => void;
  onAddTaskNote: (taskID: string) => void;
}) {
  const { session, goal, task, note, onGoal, onTask, onNote, onCreatePlan, onAddTask, onSetTaskStatus, onAddTaskNote } = props;
  const tasks = Object.values(session?.plan.tasks ?? {}).sort((left, right) => left.order - right.order);
  return (
    <div className="two-column">
      <section className="panel">
        <div className="section-title">
          <span>Plan</span>
          <span className="muted">{session?.plan.plan.goal || "none"}</span>
        </div>
        {!session?.plan.plan.id ? (
          <div className="form-stack">
            <input value={goal} onChange={(event) => onGoal(event.target.value)} placeholder="Create plan goal" />
            <button onClick={onCreatePlan}>Create plan</button>
          </div>
        ) : (
          <>
            <div className="form-stack inline">
              <input value={task} onChange={(event) => onTask(event.target.value)} placeholder="Add task description" />
              <button onClick={onAddTask}>Add task</button>
            </div>
            <div className="task-list">
              {tasks.map((item) => (
                <article key={item.id} className="task-item">
                  <div>
                    <strong>{item.description}</strong>
                    <div className="muted">{item.id}</div>
                    {session.plan.notes[item.id]?.length ? <div className="note-preview">{session.plan.notes[item.id].at(-1)}</div> : null}
                  </div>
                  <select value={item.status} onChange={(event) => onSetTaskStatus(item.id, event.target.value)}>
                    <option value="todo">todo</option>
                    <option value="doing">doing</option>
                    <option value="done">done</option>
                    <option value="blocked">blocked</option>
                  </select>
                </article>
              ))}
            </div>
          </>
        )}
      </section>
      <section className="panel">
        <div className="section-title">
          <span>Notes</span>
          <span className="muted">append note to the first selected task from the list above</span>
        </div>
        {tasks[0] ? (
          <div className="form-stack">
            <textarea value={note} onChange={(event) => onNote(event.target.value)} placeholder="Task note" />
            <button onClick={() => onAddTaskNote(tasks[0].id)}>Add note to {tasks[0].id}</button>
          </div>
        ) : (
          <p className="muted">No task selected yet.</p>
        )}
      </section>
    </div>
  );
}

function ToolsView(props: {
  approvals: PendingApprovalView[];
  commands: ShellCommandView[];
  toolLog: ToolLogEntry[];
  delegates: { delegate_id: string; status: string; task?: string }[];
  onApprove: (approvalID: string) => void;
  onDeny: (approvalID: string) => void;
  onKill: (commandID: string) => void;
}) {
  const { approvals, commands, toolLog, delegates, onApprove, onDeny, onKill } = props;
  return (
    <div className="three-stack">
      <section className="panel">
        <div className="section-title">
          <span>Pending approvals</span>
          <span className="muted">{approvals.length}</span>
        </div>
        {approvals.length === 0 ? <p className="muted">No pending approvals.</p> : approvals.map((approval) => (
          <article key={approval.approval_id} className="list-item">
            <div>
              <strong>{approval.command} {(approval.args ?? []).join(" ")}</strong>
              <div className="muted">{approval.message}</div>
            </div>
            <div className="action-row">
              <button onClick={() => onApprove(approval.approval_id)}>Approve</button>
              <button className="secondary" onClick={() => onDeny(approval.approval_id)}>Deny</button>
            </div>
          </article>
        ))}
      </section>
      <section className="panel">
        <div className="section-title">
          <span>Running commands</span>
          <span className="muted">{commands.length}</span>
        </div>
        {commands.length === 0 ? <p className="muted">No running shell commands.</p> : commands.map((command) => (
          <article key={command.command_id} className="list-item">
            <div>
              <strong>{command.command} {(command.args ?? []).join(" ")}</strong>
              <div className="muted">{command.status}</div>
              {command.last_chunk ? <pre>{command.last_chunk}</pre> : null}
            </div>
            <button className="secondary" onClick={() => onKill(command.command_id)}>Kill</button>
          </article>
        ))}
      </section>
      <section className="panel">
        <div className="section-title">
          <span>Delegates and tool log</span>
        </div>
        {delegates.length > 0 && (
          <div className="delegate-list">
            {delegates.map((delegate) => (
              <div key={delegate.delegate_id} className="delegate-item">
                <strong>{delegate.delegate_id}</strong>
                <span>{delegate.status}</span>
                {delegate.task ? <span className="muted">{delegate.task}</span> : null}
              </div>
            ))}
          </div>
        )}
        <div className="tool-log">
          {toolLog.length === 0 ? <p className="muted">No tool activity yet.</p> : toolLog.slice().reverse().map((item, index) => (
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

function SettingsView(props: {
  settings: SettingsSnapshot | null;
  draft: Record<string, unknown>;
  rawFile: SettingsRawFileContent | null;
  selectedRawPath: string;
  onDraftChange: React.Dispatch<React.SetStateAction<Record<string, unknown>>>;
  onApply: () => void;
  onSelectRaw: (path: string) => void;
  onRawChange: (content: string) => void;
  onApplyRaw: () => void;
}) {
  const { settings, draft, rawFile, selectedRawPath, onDraftChange, onApply, onSelectRaw, onRawChange, onApplyRaw } = props;
  return (
    <div className="two-column">
      <section className="panel">
        <div className="section-title">
          <span>Settings form</span>
          <span className="muted">{settings?.revision ?? "-"}</span>
        </div>
        <div className="form-grid">
          {(settings?.form_fields ?? []).map((field) => (
            <label key={field.key}>
              <span>{field.label}</span>
              {field.type === "bool" ? (
                <input
                  type="checkbox"
                  checked={Boolean(draft[field.key])}
                  onChange={(event) => onDraftChange((current) => ({ ...current, [field.key]: event.target.checked }))}
                />
              ) : (
                <input
                  value={String(draft[field.key] ?? "")}
                  onChange={(event) => onDraftChange((current) => ({ ...current, [field.key]: event.target.value }))}
                />
              )}
            </label>
          ))}
        </div>
        <button onClick={onApply}>Apply settings</button>
      </section>

      <section className="panel">
        <div className="section-title">
          <span>Raw YAML</span>
          <span className="muted">{selectedRawPath || "no file selected"}</span>
        </div>
        <select value={selectedRawPath} onChange={(event) => onSelectRaw(event.target.value)}>
          <option value="">Select raw file</option>
          {(settings?.raw_files ?? []).map((file) => (
            <option key={file.path} value={file.path}>
              {file.path}
            </option>
          ))}
        </select>
        <textarea value={rawFile?.content ?? ""} onChange={(event) => onRawChange(event.target.value)} placeholder="Raw YAML content" />
        <button onClick={onApplyRaw} disabled={!rawFile}>Apply raw file</button>
      </section>
    </div>
  );
}

function emptySessionUIState(): SessionUIState {
  return { streaming: "", status: "idle", toolLog: [], btwRuns: [] };
}

function upsertSessionSummary(current: SessionSummary[], snapshot: SessionSnapshot): SessionSummary[] {
  const next = current.filter((entry) => entry.session_id !== snapshot.session_id);
  next.unshift({
    session_id: snapshot.session_id,
    created_at: snapshot.created_at,
    last_activity: snapshot.last_activity,
    message_count: snapshot.message_count,
  });
  return next;
}

function fieldDraftFromSnapshot(settings: SettingsSnapshot): Record<string, unknown> {
  const next: Record<string, unknown> = {};
  for (const field of settings.form_fields) {
    next[field.key] = field.value;
  }
  return next;
}
