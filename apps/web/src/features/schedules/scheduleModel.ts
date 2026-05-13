import type { AgentSchedule } from "../../types";

export function scheduleStatus(schedule: AgentSchedule): "enabled" | "disabled" | "error" {
  if (schedule.last_error) {
    return "error";
  }
  return schedule.enabled ? "enabled" : "disabled";
}

export function scheduleStatusLabel(schedule: AgentSchedule): string {
  const status = scheduleStatus(schedule);
  if (status === "enabled") {
    return "включено";
  }
  if (status === "disabled") {
    return "выключено";
  }
  return "ошибка";
}

export function secondsToHuman(seconds: number): string {
  if (seconds % 86_400 === 0) {
    return `${seconds / 86_400} д`;
  }
  if (seconds % 3_600 === 0) {
    return `${seconds / 3_600} ч`;
  }
  if (seconds % 60 === 0) {
    return `${seconds / 60} мин`;
  }
  return `${seconds} сек`;
}

export function scheduleModeLabel(mode: AgentSchedule["mode"]): string {
  if (mode === "after_completion") {
    return "после завершения";
  }
  if (mode === "once") {
    return "один раз";
  }
  return "интервал";
}

export function scheduleDeliveryLabel(mode: AgentSchedule["delivery_mode"]): string {
  return mode === "existing_session" ? "в существующую сессию" : "в новую сессию";
}
