import type { SessionSummary } from "../lib/types";

export type SessionListItem = {
  id: string;
  active: boolean;
  title: string;
  meta: string;
};

export function buildSessionList(sessions: SessionSummary[], selectedSessionID: string): SessionListItem[] {
  return [...sessions]
    .sort((left, right) => right.last_activity.localeCompare(left.last_activity))
    .map((session) => ({
      id: session.session_id,
      active: session.session_id === selectedSessionID,
      title: session.session_id,
      meta: `${session.message_count} messages`,
    }));
}
