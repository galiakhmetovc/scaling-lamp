import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { PlanPane } from "./PlanPane";
import { defaultSelectedTaskID, sortedPlanTasks } from "./model";
import type { SessionSnapshot } from "../lib/types";

function makeSession(): SessionSnapshot {
  return {
    session_id: "session-1",
    created_at: "2026-04-15T21:00:00Z",
    last_activity: "2026-04-15T21:01:00Z",
    message_count: 2,
    main_run_active: false,
    main_run: { active: false, started_at: "", provider: "provider", model: "model", input_tokens: 0, output_tokens: 0, total_tokens: 0 },
    queued_drafts: [],
    history: { loaded_count: 0, total_count: 0, has_more: false, window_limit: 40 },
    base_context_tokens: 0,
    transcript: [],
    timeline: [],
    plan: {
      plan: { id: "plan-1", goal: "Ship **web** parity", status: "active", created_at: "2026-04-15T21:00:00Z" },
      tasks: {
        "task-2": { id: "task-2", plan_id: "plan-1", description: "Second task", status: "todo", order: 2 },
        "task-1": { id: "task-1", plan_id: "plan-1", description: "First `task`", status: "doing", order: 1, blocked_reason: "waiting" },
      },
      ready: { "task-1": true },
      waiting_on_dependencies: {},
      blocked: { "task-1": "waiting" },
      notes: { "task-1": ["note one", "latest **note**"] },
    },
    pending_approvals: [],
    running_commands: [],
    delegates: [],
  };
}

describe("plan model", () => {
  it("sorts tasks and picks first task as default selection", () => {
    const tasks = sortedPlanTasks(makeSession().plan);
    expect(tasks.map((task) => task.id)).toEqual(["task-1", "task-2"]);
    expect(defaultSelectedTaskID(makeSession().plan)).toBe("task-1");
  });
});

describe("PlanPane", () => {
  it("renders markdown goal, task details, and tiered surfaces", () => {
    const markup = renderToStaticMarkup(
      <PlanPane
        session={makeSession()}
        goal=""
        task=""
        note=""
        selectedTaskID="task-1"
        onGoal={() => {}}
        onTask={() => {}}
        onNote={() => {}}
        onSelectTask={() => {}}
        onCreatePlan={() => {}}
        onAddTask={() => {}}
        onSetTaskStatus={() => {}}
        onAddTaskNote={() => {}}
      />,
    );

    expect(markup).toContain("Ship");
    expect(markup).toContain("web");
    expect(markup).toContain("First");
    expect(markup).toContain("latest");
    expect(markup).toContain("waiting");
    expect(markup).toContain("surface-primary");
    expect(markup).toContain("surface-secondary");
  });
});
