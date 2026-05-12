export type ChatCommandId =
  | "new-session"
  | "refresh"
  | "open-sessions"
  | "open-status"
  | "open-tasks"
  | "open-files"
  | "open-skills"
  | "open-tools"
  | "open-debug"
  | "open-agents"
  | "open-routes"
  | "approve"
  | "autoapprove"
  | "compact"
  | "model"
  | "think"
  | "rename"
  | "stop"
  | "cancel"
  | "clear-input"
  | "send-help";

export type ChatCommand = {
  id: ChatCommandId;
  command: string;
  title: string;
  detail: string;
};

export const chatCommands: ChatCommand[] = [
  {
    id: "new-session",
    command: "/new",
    title: "Новая сессия",
    detail: "создать новую web-сессию"
  },
  {
    id: "refresh",
    command: "/refresh",
    title: "Обновить",
    detail: "перезагрузить список и transcript"
  },
  {
    id: "open-sessions",
    command: "/sessions",
    title: "Показать сессии",
    detail: "развернуть левую панель выбора сессии"
  },
  {
    id: "open-status",
    command: "/status",
    title: "Показать статус",
    detail: "развернуть правую панель состояния"
  },
  {
    id: "approve",
    command: "/approve",
    title: "Approve",
    detail: "одобрить последний ожидающий tool approval"
  },
  {
    id: "autoapprove",
    command: "/autoapprove",
    title: "Auto-approve",
    detail: "переключить auto-approve: /autoapprove on|off"
  },
  {
    id: "compact",
    command: "/compact",
    title: "Compact",
    detail: "сжать контекст выбранной сессии"
  },
  {
    id: "model",
    command: "/model",
    title: "Модель",
    detail: "показать или сменить модель: /model <name>"
  },
  {
    id: "think",
    command: "/think",
    title: "Think level",
    detail: "сменить режим размышления: /think off|low|medium|high"
  },
  {
    id: "rename",
    command: "/rename",
    title: "Переименовать",
    detail: "переименовать сессию: /rename <title>"
  },
  {
    id: "open-tasks",
    command: "/plan",
    title: "План",
    detail: "открыть task registry выбранной сессии"
  },
  {
    id: "open-files",
    command: "/files",
    title: "Файлы",
    detail: "открыть workspace и artifacts выбранной сессии"
  },
  {
    id: "open-skills",
    command: "/skills",
    title: "Skills",
    detail: "открыть навыки выбранной сессии"
  },
  {
    id: "open-tools",
    command: "/tools",
    title: "Tools",
    detail: "открыть последние tool calls"
  },
  {
    id: "open-debug",
    command: "/debug",
    title: "Debug",
    detail: "открыть timeline/debug выбранной сессии"
  },
  {
    id: "open-agents",
    command: "/agents",
    title: "Агенты",
    detail: "открыть профили агентов"
  },
  {
    id: "open-routes",
    command: "/routes",
    title: "Маршруты",
    detail: "открыть delivery targets и Telegram bindings"
  },
  {
    id: "stop",
    command: "/stop",
    title: "Остановить run",
    detail: "запросить остановку активного run"
  },
  {
    id: "cancel",
    command: "/cancel",
    title: "Отменить работу",
    detail: "запросить отмену всей работы сессии"
  },
  {
    id: "clear-input",
    command: "/clear",
    title: "Очистить ввод",
    detail: "очистить composer"
  },
  {
    id: "send-help",
    command: "/help",
    title: "Спросить help у агента",
    detail: "отправить /help в выбранную сессию"
  }
];

export function filterChatCommands(input: string): ChatCommand[] {
  const normalized = input.trim().toLowerCase();
  if (!normalized.startsWith("/")) {
    return [];
  }
  return chatCommands.filter((item) => {
    return item.command.includes(normalized) || item.title.toLowerCase().includes(normalized);
  });
}
