import type { PendingApproval, SessionSummary, SessionTask, ToolCallSummary } from "../../types";

export type WorkStatus = {
  active: boolean;
  title: string;
  detail: string;
  severity: "info" | "warning" | "error" | "success";
  toolCount: number;
  errorCount: number;
  activeTaskCount: number;
  latestTool?: ToolCallSummary;
  latestApproval?: PendingApproval;
};

function toolActionLabel(toolName: string): string {
  if (toolName.startsWith("memory_")) {
    return "Ищу в памяти";
  }
  if (toolName.startsWith("kv_")) {
    return "Работаю с KV";
  }
  if (toolName.startsWith("silverbullet_")) {
    return "Работаю с SilverBullet";
  }
  if (toolName.startsWith("web_") || toolName.startsWith("browser_")) {
    return "Ищу в вебе";
  }
  if (toolName.startsWith("fs_") || toolName === "deliver_file") {
    return "Работаю с файлами";
  }
  if (toolName.startsWith("exec_")) {
    return "Выполняю команду";
  }
  if (toolName.startsWith("schedule_") || toolName === "continue_later") {
    return "Настраиваю расписание";
  }
  if (toolName.startsWith("message_") || toolName.startsWith("agent_")) {
    return "Работаю с агентами";
  }
  return "Работаю с инструментами";
}

function readRunStatus(run: unknown): string | null {
  if (!run || typeof run !== "object") {
    return null;
  }
  const record = run as Record<string, unknown>;
  for (const key of ["status", "phase", "state"]) {
    const value = record[key];
    if (typeof value === "string" && value.length > 0) {
      return value;
    }
  }
  return null;
}

export function deriveWorkStatus({
  selectedSession,
  tools,
  tasks,
  pendingApprovals,
  run,
  sending
}: {
  selectedSession: SessionSummary | null;
  tools: ToolCallSummary[];
  tasks: SessionTask[];
  pendingApprovals: PendingApproval[];
  run: unknown;
  sending: boolean;
}): WorkStatus | null {
  if (!selectedSession && !sending) {
    return null;
  }

  const selectedTools = selectedSession ? tools.filter((tool) => tool.session_id === selectedSession.id) : [];
  const sortedTools = [...selectedTools].sort((left, right) => right.updated_at - left.updated_at);
  const latestTool = sortedTools[0];
  const activeTool = sortedTools.find((tool) => !["completed", "failed", "cancelled", "killed"].includes(tool.status));
  const errorCount = selectedTools.filter((tool) => tool.status !== "completed" || tool.error).length;
  const activeTasks = tasks.filter((task) => ["queued", "running", "in_progress"].includes(task.status));
  const latestApproval = pendingApprovals[0];
  const runStatus = readRunStatus(run);
  const runActive = Boolean(runStatus && !["completed", "failed", "cancelled", "idle"].includes(runStatus));

  if (latestApproval) {
    return {
      active: true,
      title: "Нужен approve",
      detail: latestApproval.reason || latestApproval.approval_id,
      severity: "warning",
      toolCount: selectedTools.length,
      errorCount,
      activeTaskCount: activeTasks.length,
      latestTool,
      latestApproval
    };
  }

  if (activeTool) {
    return {
      active: true,
      title: toolActionLabel(activeTool.tool_name),
      detail: activeTool.summary || activeTool.tool_name,
      severity: activeTool.error ? "error" : "info",
      toolCount: selectedTools.length,
      errorCount,
      activeTaskCount: activeTasks.length,
      latestTool: activeTool
    };
  }

  if (sending || runActive) {
    return {
      active: true,
      title: latestTool ? toolActionLabel(latestTool.tool_name) : "Думаю над ответом",
      detail: latestTool?.summary || (runStatus ? `run: ${runStatus}` : "Сообщение принято, жду ответ runtime."),
      severity: errorCount > 0 ? "warning" : "info",
      toolCount: selectedTools.length,
      errorCount,
      activeTaskCount: activeTasks.length,
      latestTool
    };
  }

  if (activeTasks.length > 0) {
    return {
      active: true,
      title: "Есть фоновые задачи",
      detail: `${activeTasks.length} активных задач в task registry`,
      severity: "info",
      toolCount: selectedTools.length,
      errorCount,
      activeTaskCount: activeTasks.length,
      latestTool
    };
  }

  return null;
}
