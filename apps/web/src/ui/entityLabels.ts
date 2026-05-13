import type { AgentSummary, DeliveryTarget, RunSummary, SessionSummary, TelegramChat } from "../types";

export type EntityLabel = {
  primary: string;
  secondary: string;
  technical: string;
};

function agentName(agentId: string | null | undefined, agents: AgentSummary[]): string | null {
  if (!agentId) {
    return null;
  }
  return agents.find((agent) => agent.id === agentId)?.name ?? agentId;
}

function findSession(sessionId: string | null | undefined, sessions: SessionSummary[]): SessionSummary | null {
  if (!sessionId) {
    return null;
  }
  return sessions.find((session) => session.id === sessionId) ?? null;
}

function shortId(value: string | null | undefined, size = 14): string {
  if (!value) {
    return "—";
  }
  return value.length > size ? `${value.slice(0, size)}…` : value;
}

export function sessionTitle(sessionId: string | null | undefined, sessions: SessionSummary[]): Pick<EntityLabel, "primary" | "secondary"> {
  const session = findSession(sessionId, sessions);
  if (!sessionId) {
    return { primary: "Сессия не выбрана", secondary: "—" };
  }
  if (!session) {
    return { primary: shortId(sessionId, 28), secondary: sessionId };
  }
  return {
    primary: session.title || shortId(session.id, 28),
    secondary: session.id
  };
}

export function describeRun(run: RunSummary, sessions: SessionSummary[]): EntityLabel {
  const session = findSession(run.session_id, sessions);
  return {
    primary: session?.title || shortId(run.session_id, 28),
    secondary: session ? `${session.agent_name} · ${session.id}` : `session: ${run.session_id}`,
    technical: run.id
  };
}

export function describeTelegramChat(chat: TelegramChat, sessions: SessionSummary[], agents: AgentSummary[]): EntityLabel {
  const session = findSession(chat.selected_session_id, sessions);
  const fallbackAgent = agentName(chat.default_agent_profile_id, agents);
  const scope = chat.scope || "chat";
  return {
    primary: session?.title || (fallbackAgent ? `${fallbackAgent} Telegram ${scope}` : `Telegram ${scope}`),
    secondary: `Telegram ${scope}${fallbackAgent ? ` · ${fallbackAgent}` : ""}`,
    technical: String(chat.telegram_chat_id)
  };
}

export function describeDeliveryTarget(target: DeliveryTarget, sessions: SessionSummary[], agents: AgentSummary[]): EntityLabel {
  const scopedSession = findSession(target.scope, sessions);
  const scopedAgent = agentName(target.scope, agents);
  const address = target.address || target.target_id;
  const kind = target.kind ? `${target.kind[0]?.toUpperCase() ?? ""}${target.kind.slice(1)}` : "Target";
  return {
    primary: `${kind} ${address}`,
    secondary: `scope: ${scopedSession?.title || scopedAgent || target.scope} · format: ${target.format_policy}`,
    technical: target.target_id
  };
}

export function describeAgentModel(agent: AgentSummary, sessions: SessionSummary[]): string {
  const latestConcreteModel = sessions
    .filter((session) => session.agent_profile_id === agent.id && Boolean(session.model))
    .sort((left, right) => right.updated_at - left.updated_at)[0]?.model;
  return latestConcreteModel || "runtime default";
}
