export type SectionId =
  | "overview"
  | "chat"
  | "sessions"
  | "files"
  | "agents"
  | "tasks"
  | "tools"
  | "routes"
  | "traces"
  | "settings";

export const sections: Array<{ id: SectionId; label: string; description: string }> = [
  { id: "overview", label: "Обзор", description: "runtime, NATS, Postgres" },
  { id: "chat", label: "Чат", description: "основная работа" },
  { id: "sessions", label: "Сессии", description: "timeline и debug" },
  { id: "files", label: "Файлы", description: "workspace и artifacts" },
  { id: "agents", label: "Агенты", description: "профили и workspaces" },
  { id: "tasks", label: "Задачи", description: "registry и делегации" },
  { id: "tools", label: "Tools", description: "вызовы и ошибки" },
  { id: "routes", label: "Маршруты", description: "delivery targets" },
  { id: "traces", label: "Traces", description: "OTel ссылки" },
  { id: "settings", label: "Настройки", description: "read-only конфиг" }
];
