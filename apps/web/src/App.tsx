import { useEffect, useState } from "react";
import { Alert, Button, Stack } from "@mui/material";
import { api } from "./api";
import { ConsoleShell } from "./components/ConsoleShell";
import { CreateAgentDialog } from "./components/CreateAgentDialog";
import { CreateSessionDialog } from "./components/CreateSessionDialog";
import { AgentsScreen } from "./features/agents/AgentsScreen";
import { ChatScreen } from "./features/chat/ChatScreen";
import { FilesScreen } from "./features/files/FilesScreen";
import { OverviewScreen } from "./features/overview/OverviewScreen";
import { RoutesView } from "./features/routes/RoutesView";
import { SessionsScreen } from "./features/sessions/SessionsScreen";
import { TasksPane } from "./features/sessions/TasksPane";
import { buildSessionEvents, type SessionPane } from "./features/sessions/sessionEvents";
import { SettingsScreen } from "./features/settings/SettingsScreen";
import { SkillsScreen } from "./features/skills/SkillsScreen";
import { ToolsScreen } from "./features/tools/ToolsScreen";
import { TracesTable } from "./features/traces/TracesTable";
import { JsonBlock, SectionHeader } from "./components/common";
import type {
  PendingApproval,
  PendingChatMessage,
  SessionDebug,
  SessionSummary,
  SessionTask,
  SessionTranscript,
  WebSnapshot
} from "./types";
import type { ChatCommand } from "./features/chat/chatCommands";
import type { SectionId } from "./ui/navigation";

const SESSION_PAGE_SIZE = 25;

export function App() {
  const [section, setSection] = useState<SectionId>("chat");
  const [snapshot, setSnapshot] = useState<WebSnapshot | null>(null);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [transcript, setTranscript] = useState<SessionTranscript | null>(null);
  const [debug, setDebug] = useState<SessionDebug | null>(null);
  const [tasks, setTasks] = useState<SessionTask[]>([]);
  const [run, setRun] = useState<unknown>(null);
  const [pendingApprovals, setPendingApprovals] = useState<PendingApproval[]>([]);
  const [pendingMessages, setPendingMessages] = useState<PendingChatMessage[]>([]);
  const [sessionPane, setSessionPane] = useState<SessionPane>("timeline");
  const [selectedEventId, setSelectedEventId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [sessionFilter, setSessionFilter] = useState("");
  const [sessionsOffset, setSessionsOffset] = useState(0);
  const [toolFilter, setToolFilter] = useState("");
  const [message, setMessage] = useState("");
  const [sending, setSending] = useState(false);
  const [approving, setApproving] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [createSessionOpen, setCreateSessionOpen] = useState(false);
  const [newSessionTitle, setNewSessionTitle] = useState("Новая web-сессия");
  const [newSessionAgent, setNewSessionAgent] = useState("");
  const [createAgentOpen, setCreateAgentOpen] = useState(false);
  const [newAgentName, setNewAgentName] = useState("");
  const [newAgentTemplate, setNewAgentTemplate] = useState("default");

  async function loadData(signal?: AbortSignal, offset = sessionsOffset, preferredSessionId?: string) {
    setLoading(true);
    setError(null);
    try {
      const [nextSnapshot, nextSessions] = await Promise.all([
        api.snapshot(signal),
        api.sessions(SESSION_PAGE_SIZE, offset, signal)
      ]);
      setSnapshot(nextSnapshot);
      setSessions(nextSessions);
      setSelectedSessionId((current) => {
        if (preferredSessionId && nextSessions.some((session) => session.id === preferredSessionId)) {
          return preferredSessionId;
        }
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

  async function loadSessionDetails(sessionId: string, signal?: AbortSignal, quiet = false) {
    if (!quiet) {
      setDetailLoading(true);
      setDetailError(null);
    }
    try {
      const [nextTranscript, nextDebug, nextTasks, nextRun, nextApprovals] = await Promise.all([
        api.transcript(sessionId, 180, signal),
        api.debug(sessionId, signal),
        api.tasks(sessionId, signal),
        api.run(sessionId, signal).catch((runError) => ({ error: runError instanceof Error ? runError.message : String(runError) })),
        api.pendingApprovals(sessionId, signal)
      ]);
      setTranscript(nextTranscript);
      setDebug(nextDebug);
      setTasks(nextTasks);
      setRun(nextRun);
      setPendingApprovals(nextApprovals);
    } catch (loadError) {
      if (!signal?.aborted) {
        setDetailError(loadError instanceof Error ? loadError.message : String(loadError));
      }
    } finally {
      if (!signal?.aborted && !quiet) {
        setDetailLoading(false);
      }
    }
  }

  useEffect(() => {
    const controller = new AbortController();
    void loadData(controller.signal, sessionsOffset);
    const timer = window.setInterval(() => void loadData(controller.signal, sessionsOffset), 10_000);
    return () => {
      controller.abort();
      window.clearInterval(timer);
    };
  }, [sessionsOffset]);

  useEffect(() => {
    if (!selectedSessionId) {
      setTranscript(null);
      setDebug(null);
      setTasks([]);
      setRun(null);
      setPendingApprovals([]);
      setSelectedEventId(null);
      return;
    }
    const controller = new AbortController();
    void loadSessionDetails(selectedSessionId, controller.signal);
    return () => controller.abort();
  }, [selectedSessionId]);

  const selectedSession = sessions.find((session) => session.id === selectedSessionId) ?? null;

  useEffect(() => {
    if (!selectedSessionId || (!sending && !approving && pendingApprovals.length === 0 && !selectedSession?.has_pending_approval)) {
      return;
    }
    const timer = window.setInterval(() => {
      void loadSessionDetails(selectedSessionId, undefined, true);
      void loadData(undefined, sessionsOffset);
    }, 2_000);
    return () => window.clearInterval(timer);
  }, [approving, pendingApprovals.length, selectedSession?.has_pending_approval, selectedSessionId, sending, sessionsOffset]);

  const sessionsTotal = snapshot?.status.session_count ?? sessions.length;
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
    const sessionId = selectedSessionId;
    const pendingId = `pending-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    setPendingMessages((current) => [
      ...current,
      {
        id: pendingId,
        session_id: sessionId,
        role: "user",
        content: trimmed,
        created_at: Math.floor(Date.now() / 1000),
        status: "sending"
      }
    ]);
    setMessage("");
    setSending(true);
    setNotice(null);
    try {
      const result = await api.sendMessage(sessionId, trimmed);
      setNotice(result.kind === "chat_completed" ? "Ответ получен, transcript обновлён." : `Runtime вернул: ${result.kind}`);
      await loadData(undefined, sessionsOffset, sessionId);
      await loadSessionDetails(sessionId);
      setPendingMessages((current) => current.filter((entry) => entry.id !== pendingId));
    } catch (sendError) {
      const errorText = sendError instanceof Error ? sendError.message : String(sendError);
      setPendingMessages((current) =>
        current.map((entry) => (entry.id === pendingId ? { ...entry, status: "failed", error: errorText } : entry))
      );
      setNotice(errorText);
    } finally {
      setSending(false);
    }
  }

  async function approveLatest(approvalId?: string) {
    if (!selectedSessionId) {
      setNotice("Сессия не выбрана.");
      return;
    }
    const sessionId = selectedSessionId;
    setApproving(true);
    setNotice(null);
    try {
      const approvals =
        pendingApprovals.length > 0 ? pendingApprovals : await api.pendingApprovals(sessionId);
      const sortedApprovals = [...approvals].sort((left, right) => right.requested_at - left.requested_at);
      const approval = approvalId
        ? sortedApprovals.find((item) => item.approval_id === approvalId)
        : sortedApprovals[0];
      if (!approval) {
        setNotice("Для выбранной сессии нет ожидающего approve.");
        return;
      }
      const outcome = await api.approveRun(approval.run_id, approval.approval_id);
      setNotice(outcome.kind === "approval_completed" ? "Approve выполнен, run продолжен." : `Runtime вернул: ${outcome.kind}`);
      await loadData(undefined, sessionsOffset, sessionId);
      await loadSessionDetails(sessionId);
    } catch (approveError) {
      setNotice(approveError instanceof Error ? approveError.message : String(approveError));
    } finally {
      setApproving(false);
    }
  }

  async function runChatCommand(command: ChatCommand, rawInput: string) {
    const arg = rawInput.trim().replace(/^\S+\s*/, "").trim();
    if (command.id === "open-tasks") {
      setSection("tasks");
      return;
    }
    if (command.id === "open-tools") {
      setSection("tools");
      return;
    }
    if (command.id === "open-files") {
      setSection("files");
      return;
    }
    if (command.id === "open-skills") {
      setSection("skills");
      return;
    }
    if (command.id === "open-debug") {
      setSection("sessions");
      return;
    }
    if (command.id === "open-agents") {
      setSection("agents");
      return;
    }
    if (command.id === "open-routes") {
      setSection("routes");
      return;
    }
    if (command.id === "send-help") {
      setNotice("Команды web: /new, /sessions, /status, /approve, /stop, /cancel, /autoapprove, /compact, /model, /think, /rename, /plan, /files, /skills, /tools, /debug.");
      return;
    }
    if (!selectedSessionId) {
      setNotice("Сессия не выбрана.");
      return;
    }

    try {
      if (command.id === "approve") {
        await approveLatest(arg || undefined);
        return;
      }
      if (command.id === "compact") {
        const summary = await api.compactSession(selectedSessionId);
        setNotice(`Context compact выполнен. Сжатий: ${summary.compactifications}.`);
        await loadData(undefined, sessionsOffset, selectedSessionId);
        await loadSessionDetails(selectedSessionId);
        return;
      }
      if (command.id === "autoapprove") {
        const normalized = arg.toLowerCase();
        const nextValue =
          normalized === "on" || normalized === "true" || normalized === "1"
            ? true
            : normalized === "off" || normalized === "false" || normalized === "0"
              ? false
              : !selectedSession?.auto_approve;
        const summary = await api.updateSessionPreferences(selectedSessionId, { auto_approve: nextValue });
        setNotice(`Auto-approve: ${summary.auto_approve ? "включён" : "выключен"}.`);
        await loadData(undefined, sessionsOffset, selectedSessionId);
        return;
      }
      if (command.id === "model") {
        if (!arg) {
          setNotice(`Текущая модель: ${selectedSession?.model || "default"}. Для смены: /model <name>.`);
          return;
        }
        const summary = await api.updateSessionPreferences(selectedSessionId, { model: arg === "default" ? null : arg });
        setNotice(`Модель: ${summary.model || "default"}.`);
        await loadData(undefined, sessionsOffset, selectedSessionId);
        return;
      }
      if (command.id === "think") {
        if (!arg) {
          setNotice(`Think level: ${selectedSession?.think_level || "default"}. Для смены: /think off|low|medium|high|default.`);
          return;
        }
        const nextThink = arg === "default" ? null : arg;
        const summary = await api.updateSessionPreferences(selectedSessionId, { think_level: nextThink });
        setNotice(`Think level: ${summary.think_level || "default"}.`);
        await loadData(undefined, sessionsOffset, selectedSessionId);
        return;
      }
      if (command.id === "rename") {
        if (!arg) {
          setNotice("Укажи новое имя: /rename <title>.");
          return;
        }
        const summary = await api.updateSessionPreferences(selectedSessionId, { title: arg });
        setNotice(`Сессия переименована: ${summary.title}.`);
        await loadData(undefined, sessionsOffset, selectedSessionId);
      }
    } catch (commandError) {
      setNotice(commandError instanceof Error ? commandError.message : String(commandError));
    }
  }

  async function submitCreateSession() {
    const title = newSessionTitle.trim() || "Новая web-сессия";
    try {
      const created = await api.createSession(title, newSessionAgent || undefined);
      setCreateSessionOpen(false);
      setSelectedSessionId(created.id);
      setSessionsOffset(0);
      setSection("chat");
      setNotice(`Сессия создана: ${created.title}`);
      await loadData(undefined, 0, created.id);
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
      await loadData(undefined, sessionsOffset);
    } catch (cancelError) {
      setNotice(cancelError instanceof Error ? cancelError.message : String(cancelError));
    }
  }

  function refreshSelectedSession() {
    if (selectedSessionId) {
      void loadSessionDetails(selectedSessionId);
    }
    void loadData(undefined, sessionsOffset);
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
            debug={debug}
            tasks={tasks}
            tools={snapshot?.recent_tool_calls ?? []}
            run={run}
            pendingMessages={pendingMessages}
            pendingApprovals={pendingApprovals}
            sessionFilter={sessionFilter}
            sessionsTotal={sessionsTotal}
            sessionsOffset={sessionsOffset}
            sessionsLimit={SESSION_PAGE_SIZE}
            message={message}
            loading={loading}
            detailLoading={detailLoading}
            detailError={detailError}
            sending={sending}
            approving={approving}
            onRefresh={refreshSelectedSession}
            onCreateSession={() => setCreateSessionOpen(true)}
            onSelectSession={setSelectedSessionId}
            onFilterChange={setSessionFilter}
            onSessionsPageChange={setSessionsOffset}
            onMessageChange={setMessage}
            onSend={() => void submitMessage()}
            onCommand={(command, rawInput) => void runChatCommand(command, rawInput)}
            onApprove={(approvalId) => void approveLatest(approvalId)}
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
            sessionsTotal={sessionsTotal}
            sessionsOffset={sessionsOffset}
            sessionsLimit={SESSION_PAGE_SIZE}
            message={message}
            loading={loading}
            detailLoading={detailLoading}
            detailError={detailError}
            sending={sending}
            onRefresh={refreshSelectedSession}
            onCreateSession={() => setCreateSessionOpen(true)}
            onSelectSession={setSelectedSessionId}
            onFilterChange={setSessionFilter}
            onSessionsPageChange={setSessionsOffset}
            onPaneChange={setSessionPane}
            onSelectEvent={setSelectedEventId}
            onMessageChange={setMessage}
            onSend={() => void submitMessage()}
            onCancelRun={() => void cancelRun(false)}
            onCancelAll={() => void cancelRun(true)}
          />
        );
      case "files":
        return <FilesScreen selectedSession={selectedSession} />;
      case "skills":
        return <SkillsScreen selectedSession={selectedSession} />;
      case "agents":
        return (
          <AgentsScreen
            agents={snapshot?.agents ?? []}
            sessions={sessions}
            loading={loading}
            onCreateAgent={() => setCreateAgentOpen(true)}
            onOpenSession={(sessionId) => {
              setSelectedSessionId(sessionId);
              setSection("chat");
            }}
            onRefresh={() => void loadData()}
          />
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
          <ToolsScreen
            selectedSession={selectedSession}
            recentTools={snapshot?.recent_tool_calls ?? []}
            filter={toolFilter}
            onFilterChange={setToolFilter}
          />
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
      sessionsLength={sessionsTotal}
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
