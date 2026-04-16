import type { SessionSummary } from "../lib/types";

export type SessionListItem = {
  id: string;
  active: boolean;
  title: string;
  meta: string;
  activityText: string;
};

export function buildSessionList(sessions: SessionSummary[], selectedSessionID: string): SessionListItem[] {
  return [...sessions]
    .sort((left, right) => right.last_activity.localeCompare(left.last_activity))
    .map((session) => ({
      id: session.session_id,
      active: session.session_id === selectedSessionID,
      title: session.session_id,
      meta: `${session.message_count} messages`,
      activityText: formatLastActivity(session.last_activity),
    }));
}

export function formatLastActivity(timestamp: string): string {
  if (!timestamp) {
    return "no activity";
  }
  const date = new Date(timestamp);
  return `active ${date.toLocaleString()}`;
}
