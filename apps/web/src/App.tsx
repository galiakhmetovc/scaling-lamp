import { useEffect, useState } from "react";
import { Alert, Button, Stack } from "@mui/material";
import { api } from "./api";
import { ConsoleShell } from "./components/ConsoleShell";
import { CreateAgentDialog } from "./components/CreateAgentDialog";
import { CreateSessionDialog } from "./components/CreateSessionDialog";
import { AgentsTable } from "./features/agents/AgentsTable";
import { ChatScreen } from "./features/chat/ChatScreen";
import { OverviewScreen } from "./features/overview/OverviewScreen";
import { RoutesView } from "./features/routes/RoutesView";
import { SessionsScreen } from "./features/sessions/SessionsScreen";
import { TasksPane } from "./features/sessions/TasksPane";
import { buildSessionEvents, type SessionPane } from "./features/sessions/sessionEvents";
import { SettingsScreen } from "./features/settings/SettingsScreen";
import { ToolsTable } from "./features/tools/ToolsTable";
import { TracesTable } from "./features/traces/TracesTable";
import { JsonBlock, SectionHeader } from "./components/common";
import type { SessionDebug, SessionSummary, SessionTask, SessionTranscript, WebSnapshot } from "./types";
import type { SectionId } from "./ui/navigation";

export function App() {
  const [section, setSection] = useState<SectionId>("chat");
  const [snapshot, setSnapshot] = useState<WebSnapshot | null>(null);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [transcript, setTranscript] = useState<SessionTranscript | null>(null);
  const [debug, setDebug] = useState<SessionDebug | null>(null);
  const [tasks, setTasks] = useState<SessionTask[]>([]);
  const [run, setRun] = useState<unknown>(null);
  const [sessionPane, setSessionPane] = useState<SessionPane>("timeline");
  const [selectedEventId, setSelectedEventId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [sessionFilter, setSessionFilter] = useState("");
  const [toolFilter, setToolFilter] = useState("");
  const [message, setMessage] = useState("");
  const [sending, setSending] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [createSessionOpen, setCreateSessionOpen] = useState(false);
  const [newSessionTitle, setNewSessionTitle] = useState("Новая web-сессия");
  const [newSessionAgent, setNewSessionAgent] = useState("");
  const [createAgentOpen, setCreateAgentOpen] = useState(false);
  const [newAgentName, setNewAgentName] = useState("");
  const [newAgentTemplate, setNewAgentTemplate] = useState("default");

  async function loadData(signal?: AbortSignal) {
    setLoading(true);
    setError(null);
    try {
      const [nextSnapshot, nextSessions] = await Promise.all([api.snapshot(signal), api.sessions(signal)]);
      setSnapshot(nextSnapshot);
      setSessions(nextSessions);
      setSelectedSessionId((current) => {
        if (current && nextSessions.some((session) => session.id === current)) {
          return current;
        }
        return nextSessions[0]?.id ?? null;
      });
    } catch (loadError) {
      if (!signal?.aborted) {
        setError(loadError instanceof Error ? loadError.message : String(loadError));
      }
    } finally {
      if (!signal?.aborted) {
        setLoading(false);
      }
    }
  }

  async function loadSessionDetails(sessionId: string, signal?: AbortSignal) {
    setDetailLoading(true);
    setDetailError(null);
    try {
      const [nextTranscript, nextDebug, nextTasks, nextRun] = await Promise.all([
        api.transcript(sessionId, 180, signal),
        api.debug(sessionId, signal),
        api.tasks(sessionId, signal),
        api.run(sessionId, signal).catch((runError) => ({ error: runError instanceof Error ? runError.message : String(runError) }))
      ]);
      setTranscript(nextTranscript);
      setDebug(nextDebug);
      setTasks(nextTasks);
      setRun(nextRun);
    } catch (loadError) {
      if (!signal?.aborted) {
        setDetailError(loadError instanceof Error ? loadError.message : String(loadError));
      }
    } finally {
      if (!signal?.aborted) {
        setDetailLoading(false);
      }
    }
  }

  useEffect(() => {
    const controller = new AbortController();
    void loadData(controller.signal);
    const timer = window.setInterval(() => void loadData(controller.signal), 10_000);
    return () => {
      controller.abort();
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (!selectedSessionId) {
      setTranscript(null);
      setDebug(null);
      setTasks([]);
      setRun(null);
      setSelectedEventId(null);
      return;
    }
    const controller = new AbortController();
    void loadSessionDetails(selectedSessionId, controller.signal);
    return () => controller.abort();
  }, [selectedSessionId]);

  const selectedSession = sessions.find((session) => session.id === selectedSessionId) ?? null;
  const toolErrors = snapshot?.recent_tool_calls.filter((tool) => tool.status !== "completed" || tool.error).length ?? 0;
  const activeRuns = snapshot?.recent_runs.filter((runItem) => ["running", "queued"].includes(runItem.status)).length ?? 0;
  const sessionEvents = buildSessionEvents(debug, transcript);
  const selectedEvent =
    sessionEvents.find((event) => event.id === selectedEventId) ?? sessionEvents[sessionEvents.length - 1] ?? null;

  useEffect(() => {
    if (sessionEvents.length === 0) {
      setSelectedEventId(null);
      return;
    }
    setSelectedEventId((current) => {
      if (current && sessionEvents.some((event) => event.id === current)) {
        return current;
      }
      return sessionEvents[sessionEvents.length - 1].id;
    });
  }, [debug, transcript]);

  async function submitMessage() {
    const trimmed = message.trim();
    if (!selectedSessionId || !trimmed) {
      return;
    }
    setSending(true);
    setNotice(null);
    try {
      const result = await api.sendMessage(selectedSessionId, trimmed);
      setMessage("");
      setNotice(result.kind === "chat_completed" ? "Ответ получен, transcript обновлён." : `Runtime вернул: ${result.kind}`);
      await loadData();
      await loadSessionDetails(selectedSessionId);
    } catch (sendError) {
      setNotice(sendError instanceof Error ? sendError.message : String(sendError));
    } finally {
      setSending(false);
    }
  }

  async function submitCreateSession() {
    const title = newSessionTitle.trim() || "Новая web-сессия";
    try {
      const created = await api.createSession(title, newSessionAgent || undefined);
      setCreateSessionOpen(false);
      setSelectedSessionId(created.id);
      setSection("chat");
      setNotice(`Сессия создана: ${created.title}`);
      await loadData();
    } catch (createError) {
      setNotice(createError instanceof Error ? createError.message : String(createError));
    }
  }

  async function submitCreateAgent() {
    const name = newAgentName.trim();
    if (!name) {
      setNotice("Укажи имя агента.");
      return;
    }
    try {
      const result = await api.createAgent(name, newAgentTemplate || undefined);
      setCreateAgentOpen(false);
      setNewAgentName("");
      setNotice(result.message);
      await loadData();
    } catch (createError) {
      setNotice(createError instanceof Error ? createError.message : String(createError));
    }
  }

  async function cancelRun(all: boolean) {
    if (!selectedSessionId) {
      return;
    }
    try {
      await (all ? api.cancelAllWork(selectedSessionId) : api.cancelRun(selectedSessionId));
      setNotice(all ? "Запрошена отмена всей работы сессии." : "Запрошена отмена активного run.");
      await loadSessionDetails(selectedSessionId);
      await loadData();
    } catch (cancelError) {
      setNotice(cancelError instanceof Error ? cancelError.message : String(cancelError));
    }
  }

  function refreshSelectedSession() {
    if (selectedSessionId) {
      void loadSessionDetails(selectedSessionId);
    }
    void loadData();
  }

  function renderContent() {
    if (error) {
      return (
        <Stack spacing={2}>
          <Alert severity="error">{error}</Alert>
          <Button variant="contained" onClick={() => void loadData()}>
            Повторить
          </Button>
        </Stack>
      );
    }

    switch (section) {
      case "overview":
        return (
          <OverviewScreen
            snapshot={snapshot}
            sessions={sessions}
            loading={loading}
            toolErrors={toolErrors}
            activeRuns={activeRuns}
            onRefresh={() => void loadData()}
          />
        );
      case "chat":
        return (
          <ChatScreen
            sessions={sessions}
            selectedSession={selectedSession}
            selectedSessionId={selectedSessionId}
            transcript={transcript}
            tasks={tasks}
            tools={snapshot?.recent_tool_calls ?? []}
            sessionFilter={sessionFilter}
            message={message}
            loading={loading}
            detailLoading={detailLoading}
            detailError={detailError}
            sending={sending}
            onRefresh={() => void loadData()}
            onCreateSession={() => setCreateSessionOpen(true)}
            onSelectSession={setSelectedSessionId}
            onFilterChange={setSessionFilter}
            onMessageChange={setMessage}
            onSend={() => void submitMessage()}
            onCancelRun={() => void cancelRun(false)}
            onCancelAll={() => void cancelRun(true)}
          />
        );
      case "sessions":
        return (
          <SessionsScreen
            sessions={sessions}
            selectedSession={selectedSession}
            selectedSessionId={selectedSessionId}
            transcript={transcript}
            debug={debug}
            tasks={tasks}
            tools={snapshot?.recent_tool_calls ?? []}
            run={run}
            sessionPane={sessionPane}
            sessionEvents={sessionEvents}
            selectedEvent={selectedEvent}
            sessionFilter={sessionFilter}
            message={message}
            loading={loading}
            detailLoading={detailLoading}
            detailError={detailError}
            sending={sending}
            onRefresh={refreshSelectedSession}
            onCreateSession={() => setCreateSessionOpen(true)}
            onSelectSession={setSelectedSessionId}
            onFilterChange={setSessionFilter}
            onPaneChange={setSessionPane}
            onSelectEvent={setSelectedEventId}
            onMessageChange={setMessage}
            onSend={() => void submitMessage()}
            onCancelRun={() => void cancelRun(false)}
            onCancelAll={() => void cancelRun(true)}
          />
        );
      case "agents":
        return (
          <>
            <SectionHeader title="Агенты" subtitle="Agent profiles из canonical runtime." />
            <AgentsTable agents={snapshot?.agents ?? []} onCreate={() => setCreateAgentOpen(true)} />
          </>
        );
      case "tasks":
        return (
          <>
            <SectionHeader
              title="Task registry"
              subtitle={selectedSession ? `Задачи выбранной сессии: ${selectedSession.title}` : "Выбери сессию на вкладке Сессии."}
            />
            <TasksPane tasks={tasks} />
          </>
        );
      case "tools":
        return (
          <>
            <SectionHeader title="Tool calls" subtitle="Последние вызовы инструментов из /v1/web/snapshot." />
            <ToolsTable tools={snapshot?.recent_tool_calls ?? []} filter={toolFilter} onFilterChange={setToolFilter} />
          </>
        );
      case "routes":
        return (
          <>
            <SectionHeader title="Маршруты доставки" subtitle="Delivery targets и Telegram bindings." />
            <RoutesView targets={snapshot?.delivery_targets ?? []} chats={snapshot?.telegram_chats ?? []} />
          </>
        );
      case "traces":
        return (
          <>
            <SectionHeader title="Traces" subtitle="Ссылки на trace/span, которые можно сопоставлять с Jaeger/OTel." />
            <TracesTable traces={snapshot?.recent_traces ?? []} />
          </>
        );
      case "settings":
        return <SettingsScreen snapshot={snapshot} />;
      default:
        return <JsonBlock value={{ error: "unknown section", section }} />;
    }
  }

  return (
    <ConsoleShell
      section={section}
      snapshot={snapshot}
      sessionsLength={sessions.length}
      toolErrors={toolErrors}
      loading={loading}
      onSectionChange={setSection}
      overlays={
        <>
          <CreateSessionDialog
            open={createSessionOpen}
            title={newSessionTitle}
            agent={newSessionAgent}
            agents={snapshot?.agents ?? []}
            onClose={() => setCreateSessionOpen(false)}
            onTitleChange={setNewSessionTitle}
            onAgentChange={setNewSessionAgent}
            onSubmit={() => void submitCreateSession()}
          />
          <CreateAgentDialog
            open={createAgentOpen}
            name={newAgentName}
            template={newAgentTemplate}
            onClose={() => setCreateAgentOpen(false)}
            onNameChange={setNewAgentName}
            onTemplateChange={setNewAgentTemplate}
            onSubmit={() => void submitCreateAgent()}
          />
          {notice ? (
            <Alert
              severity={notice.toLowerCase().includes("error") || notice.includes("Ошибка") ? "error" : "info"}
              onClose={() => setNotice(null)}
              className="notice"
            >
              {notice}
            </Alert>
          ) : null}
        </>
      }
    >
      {renderContent()}
    </ConsoleShell>
  );
}
