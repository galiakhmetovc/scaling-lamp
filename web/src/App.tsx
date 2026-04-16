import { useEffect, useMemo, useRef, useState } from "react";
import { ChatPane } from "./chat/ChatPane";
import {
  appendBtwRun,
  applySessionUIEvent,
  emptySessionUIState,
  mergeSessionHistory,
  markMainRunStarted,
  prependOlderTimeline,
  resolveBtwRun,
  storeMainRunResult,
  syncMainRunFromUIEvent,
  type SessionUIState,
} from "./chat/model";
import { defaultSelectedTaskID } from "./plan/model";
import { PlanPane } from "./plan/PlanPane";
import { SessionsPane } from "./sessions/SessionsPane";
import { sessionSelectionIntent } from "./sessions/model";
import { SettingsPane } from "./settings/SettingsPane";
import { ToolsPane } from "./tools/ToolsPane";
import { DaemonClient, loadRuntimeClientConfig } from "./lib/client";
import { buildControlHeaderView, tabs, type TabKey } from "./layout";
import type {
  BootstrapPayload,
  SessionSnapshot,
  SessionSummary,
  SettingsFieldState,
  SettingsRawFileContent,
  SettingsSnapshot,
  UIEvent,
  WebsocketEnvelope,
} from "./lib/types";

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
        const bootSessions = boot.sessions ?? [];
        setBootstrap({ ...boot, sessions: bootSessions });
        setSessions(bootSessions);
        setSettings(boot.settings);
        setSettingsDraft(fieldDraftFromSnapshot(boot.settings));
        if (bootSessions.length > 0) {
          setSelectedSessionID((current) => current || bootSessions[0].session_id);
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
  const headerView = useMemo(
    () => buildControlHeaderView({ bootstrap, connected, selectedSession, errorMessage }),
    [bootstrap, connected, selectedSession, errorMessage],
  );

  useEffect(() => {
    if (!selectedSession?.main_run.active) {
      setClockNow(new Date());
      return;
    }
    const timer = window.setInterval(() => setClockNow(new Date()), 1000);
    return () => window.clearInterval(timer);
  }, [selectedSession?.main_run.active, selectedSession?.main_run.started_at]);

  function applySessionSnapshot(snapshot: SessionSnapshot) {
    setSessionSnapshots((current) => ({
      ...current,
      [snapshot.session_id]: mergeSessionHistory(current[snapshot.session_id], snapshot),
    }));
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
      if (envelope.event.kind === "tool.completed") {
        void refreshSession(envelope.event.session_id);
      }
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

  function handleSelectSession(sessionID: string) {
    const intent = sessionSelectionIntent(sessionID);
    setSelectedSessionID(intent.sessionID);
    setActiveTab(intent.nextTab);
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

  async function handleLoadOlderHistory() {
    const client = clientRef.current;
    if (!client || !selectedSession) {
      return;
    }
    const result = await client.command("session.history", {
      session_id: selectedSession.session_id,
      loaded_count: selectedSession.history.loaded_count,
      history_limit: selectedSession.history.window_limit,
    });
    setSessionSnapshots((current) => {
      const session = current[selectedSession.session_id];
      if (!session) {
        return current;
      }
      return {
        ...current,
        [selectedSession.session_id]: prependOlderTimeline(session, result),
      };
    });
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

  async function handleQuickControlChange(key: string, value: unknown) {
    const client = clientRef.current;
    if (!client || !settings) {
      return;
    }
    const nextValues = { ...settingsDraft, [key]: value };
    setSettingsDraft(nextValues);
    try {
      const result = await client.command("settings.form.apply", {
        base_revision: settings.revision,
        values: nextValues,
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
        content: rawDraft,
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
      <header className="control-header surface surface-utility">
        <div className="control-header-main">
          <div className="brand-lockup">
            <div className="eyebrow">{headerView.eyebrow}</div>
            <h1>{headerView.title}</h1>
          </div>
          <div className="active-session-badge">
            <strong>{headerView.sessionLabel}</strong>
            <span>{headerView.sessionMeta}</span>
          </div>
        </div>
        <div className="control-header-meta">
          {headerView.statusChips.map((chip) => (
            <span key={chip} className={`status-pill ${chip === "websocket up" ? "ok" : ""} ${chip === errorMessage ? "danger" : ""}`}>
              {chip}
            </span>
          ))}
        </div>
        <nav className="control-tabs">
          {tabs.map((tab) => (
            <button key={tab.key} className={tab.key === activeTab ? "active" : ""} onClick={() => setActiveTab(tab.key)}>
              {tab.label}
            </button>
          ))}
        </nav>
      </header>

      <main className={`workspace workspace-${activeTab}`}>
        <section className="main-panel">
          {activeTab === "sessions" && (
            <SessionsPane
              bootstrap={bootstrap}
              sessions={sessions}
              selectedSessionID={selectedSessionID}
              onSelectSession={handleSelectSession}
              onCreateSession={() => void handleCreateSession()}
            />
          )}
          {activeTab === "chat" && (
            <ChatPane
              session={selectedSession}
              streaming={selectedUI.streaming}
              status={selectedUI.status}
              input={chatInput}
              now={clockNow}
              btwRuns={selectedUI.btwRuns}
              toolLog={selectedUI.toolLog}
              onInput={setChatInput}
              onSend={() => void handleSendChat()}
              onQueue={() => void handleQueueDraft()}
              onRecallDraft={(id) => void handleRecallDraft(id)}
              onLoadOlder={() => void handleLoadOlderHistory()}
              quickControls={buildQuickControls(settings, settingsDraft)}
              settingsError={settingsError}
              onQuickControlChange={(key, value) => void handleQuickControlChange(key, value)}
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
              }}
              onApplyRaw={() => void handleApplyRaw()}
            />
          )}
        </section>
      </main>
    </div>
  );
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

function buildQuickControls(settings: SettingsSnapshot | null, draft: Record<string, unknown>): SettingsFieldState[] {
  if (!settings) {
    return [];
  }
  return settings.quick_controls.map((field) => ({
    ...field,
    value: Object.prototype.hasOwnProperty.call(draft, field.key) ? draft[field.key] : field.value,
  }));
}
