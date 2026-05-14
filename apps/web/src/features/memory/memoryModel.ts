export const memoryScopes = ["operator", "agent", "agent_shared"] as const;

export type MemoryScope = (typeof memoryScopes)[number];

export function memoryScopeRequiresSession(scope: MemoryScope): boolean {
  return scope === "agent";
}

export function describeMemoryScope(scope: MemoryScope): string {
  if (scope === "operator") {
    return "Долгосрочная память об операторе: предпочтения, рабочие привычки, постоянный личный контекст.";
  }
  if (scope === "agent") {
    return "Опыт и настройки конкретного агента. В текущем API определяется через agent profile выбранной context session.";
  }
  return "Общая память runtime/роя: факты и договорённости, доступные всем агентам.";
}

export function jsonPreview(value: unknown, maxLength = 180): string {
  const rendered = typeof value === "string" ? value : JSON.stringify(value);
  if (!rendered) {
    return "null";
  }
  return rendered.length > maxLength ? `${rendered.slice(0, maxLength)}…` : rendered;
}

export function parseJsonInput(value: string): unknown {
  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }
  return JSON.parse(trimmed);
}

export function describeMemoryLayer(layer: "mem0" | "kv" | "silverbullet"): string {
  if (layer === "mem0") {
    return "Mem0: семантическая память для fuzzy recall. Подмешивается в prompt ограниченным Memory Recall блоком.";
  }
  if (layer === "kv") {
    return "KV: точное scoped key-value состояние для агентов, сессий, workspace и operator.";
  }
  return "SilverBullet: человекочитаемые заметки, журналы и проектные документы. Это knowledge base, не hidden memory.";
}
