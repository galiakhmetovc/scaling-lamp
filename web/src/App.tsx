import { useEffect, useMemo, useRef, useState } from "react";
import { ChatPane } from "./chat/ChatPane";
import {
  appendBtwRun,
  applySessionUIEvent,
  approximateContextTokens,
  emptySessionUIState,
  markMainRunStarted,
  resolveBtwRun,
  storeMainRunResult,
  syncMainRunFromUIEvent,
  type SessionUIState,
} from "./chat/model";
import { defaultSelectedTaskID } from "./plan/model";
import { PlanPane } from "./plan/PlanPane";
import { SettingsPane } from "./settings/SettingsPane";
import { ToolsPane } from "./tools/ToolsPane";
import { DaemonClient, loadRuntimeClientConfig } from "./lib/client";
import type {
  BootstrapPayload,
  SessionSnapshot,
  SessionSummary,
  SettingsRawFileContent,
  SettingsSnapshot,
  UIEvent,
  WebsocketEnvelope,
} from "./lib/types";

type TabKey = "sessions" | "chat" | "plan" | "tools" | "settings";

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
  const [selectedPlanTaskID, setSelectedPlanTaskID] = useState("");
  const [settings, setSettings] = useState<SettingsSnapshot | null>(null);
  const [settingsDraft, setSettingsDraft] = useState<Record<string, unknown>>({});
  const [rawFile, setRawFile] = useState<SettingsRawFileContent | null>(null);
  const [rawDraft, setRawDraft] = useState("");
  const [selectedRawPath, setSelectedRawPath] = useState("");
  const [settingsError, setSettingsError] = useState("");
  const [clockNow, setClockNow] = useState(() => new Date());
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

  useEffect(() => {
    const timer = window.setInterval(() => setClockNow(new Date()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const selectedSession = selectedSessionID ? sessionSnapshots[selectedSessionID] : null;
  const selectedUI = selectedSessionID ? sessionUI[selectedSessionID] ?? emptySessionUIState() : emptySessionUIState();
  const approxTokens = useMemo(() => approximateContextTokens(selectedSession, chatInput), [selectedSession, chatInput]);

  function applySessionSnapshot(snapshot: SessionSnapshot) {
    setSessionSnapshots((current) => ({ ...current, [snapshot.session_id]: snapshot }));
    setSessions((current) => upsertSessionSummary(current, snapshot));
    setSelectedSessionID((current) => current || snapshot.session_id);
    setSelectedPlanTaskID((current) => current || defaultSelectedTaskID(snapshot.plan));
  }

  function applyUIEvent(event: UIEvent) {
    if (!event.session_id) {
      return;
    }
    setSessionUI((current) => ({ ...current, [event.session_id]: applySessionUIEvent(current[event.session_id], event) }));
  }

  function handleEnvelope(envelope: WebsocketEnvelope) {
    if (envelope.type === "ui_event" && envelope.event) {
      applyUIEvent(envelope.event);
      setSessionSnapshots((current) => {
        const existing = current[envelope.event!.session_id];
        const next = syncMainRunFromUIEvent(existing ?? null, envelope.event!, new Date());
        if (!next) {
          return current;
        }
        return { ...current, [next.session_id]: next };
      });
      if (envelope.event.kind === "run.completed") {
        void refreshSession(envelope.event.session_id);
      }
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
    setRawDraft(result.file.content);
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
        setSessionUI((current) => ({ ...current, [selectedSessionID]: appendBtwRun(current[selectedSessionID], { id: runID, prompt: btwPrompt, active: true }) }));
        setChatInput("");
        try {
          const result = await client.command("chat.btw", { session_id: selectedSessionID, prompt: btwPrompt });
          setSessionUI((current) => ({ ...current, [selectedSessionID]: resolveBtwRun(current[selectedSessionID], runID, { active: false, result: result.result }) }));
        } catch (error) {
          setSessionUI((current) => ({ ...current, [selectedSessionID]: resolveBtwRun(current[selectedSessionID], runID, { active: false, error: String(error) }) }));
        }
      }
      return;
    }
    setChatInput("");
    try {
      if (selectedSession) {
        applySessionSnapshot(markMainRunStarted(selectedSession, new Date()));
      }
      const result = await client.command("chat.send", { session_id: selectedSessionID, prompt });
      applySessionSnapshot(result.session);
      if (result.result) {
        setSessionUI((current) => ({ ...current, [selectedSessionID]: storeMainRunResult(current[selectedSessionID], result.result) }));
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
    try {
      const result = await client.command("settings.form.apply", {
        base_revision: settings.revision,
        values: settingsDraft,
      });
      setSettings(result.settings);
      setSettingsDraft(fieldDraftFromSnapshot(result.settings));
      setSettingsError("");
      setStatusMessage("settings applied");
    } catch (error) {
      setSettingsError(String(error));
    }
  }

  async function handleApplyRaw() {
    const client = clientRef.current;
    if (!client || !rawFile) {
      return;
    }
    try {
      const result = await client.command("settings.raw.apply", {
        path: rawFile.path,
        base_revision: rawFile.revision,
        content: rawFile.content,
      });
      setSettings(result.settings);
      setSettingsDraft(fieldDraftFromSnapshot(result.settings));
      await loadRawFile(rawFile.path);
      setSettingsError("");
      setStatusMessage("raw config applied");
    } catch (error) {
      setSettingsError(String(error));
    }
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
            <ChatPane
              session={selectedSession}
              streaming={selectedUI.streaming}
              status={selectedUI.status}
              input={chatInput}
              now={clockNow}
              btwRuns={selectedUI.btwRuns}
              onInput={setChatInput}
              onSend={() => void handleSendChat()}
              onQueue={() => void handleQueueDraft()}
              onRecallDraft={(id) => void handleRecallDraft(id)}
            />
          )}
          {activeTab === "plan" && (
            <PlanPane
              session={selectedSession}
              goal={planGoal}
              task={planTask}
              note={planNote}
              selectedTaskID={selectedPlanTaskID}
              onGoal={setPlanGoal}
              onTask={setPlanTask}
              onNote={setPlanNote}
              onSelectTask={setSelectedPlanTaskID}
              onCreatePlan={() => void handleCreatePlan()}
              onAddTask={() => void handleAddTask()}
              onSetTaskStatus={(taskID, status) => void handleSetTaskStatus(taskID, status)}
              onAddTaskNote={(taskID) => void handleAddTaskNote(taskID)}
            />
          )}
          {activeTab === "tools" && (
            <ToolsPane
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
            <SettingsPane
              settings={settings}
              draft={settingsDraft}
              rawFile={rawFile}
              selectedRawPath={selectedRawPath}
              rawDraft={rawDraft}
              error={settingsError}
              onDraftChange={setSettingsDraft}
              onApply={() => void handleApplySettings()}
              onSelectRaw={(path) => setSelectedRawPath(path)}
              onRawChange={(content) => {
                setRawDraft(content);
                setRawFile((current) => (current ? { ...current, content } : current));
              }}
              onApplyRaw={() => void handleApplyRaw()}
            />
          )}
        </section>
      </main>

      <footer className="statusline">
        <span>{statusMessage}</span>
        {selectedSession && <span>queue {selectedSession.queued_drafts.length}</span>}
        {selectedSession && <span>tokens ~{approxTokens}</span>}
        {selectedSession && <span>{selectedSession.main_run.provider}</span>}
        {selectedSession && <span>{selectedSession.main_run.model}</span>}
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
