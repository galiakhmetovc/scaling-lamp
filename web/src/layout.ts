export type TabKey = "sessions" | "chat" | "plan" | "tools" | "settings";

export function shouldShowSessionRail(tab: TabKey): boolean {
  return tab === "sessions";
}
