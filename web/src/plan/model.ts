import type { PlanHeadSnapshot, PlanTaskView } from "../lib/types";

export function sortedPlanTasks(plan: PlanHeadSnapshot): PlanTaskView[] {
  return Object.values(plan.tasks ?? {}).sort((left, right) => left.order - right.order);
}

export function defaultSelectedTaskID(plan: PlanHeadSnapshot): string {
  return sortedPlanTasks(plan)[0]?.id ?? "";
}

export function latestTaskNote(plan: PlanHeadSnapshot, taskID: string): string {
  const notes = plan.notes?.[taskID] ?? [];
  return notes[notes.length - 1] ?? "";
}
