export type ChatCommandId =
  | "new-session"
  | "refresh"
  | "open-sessions"
  | "open-status"
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
