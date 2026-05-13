import assert from "node:assert/strict";
import test from "node:test";
import type { WebSnapshot } from "../../types.ts";
import { recentActiveRuns, summarizeOperations } from "./operationsModel.ts";

function snapshot(overrides: Partial<WebSnapshot>): WebSnapshot {
  return {
    generated_at: 1,
    status: {
      ok: true,
      permission_mode: "full",
      session_count: 0,
      mission_count: 0,
      run_count: 0,
      job_count: 0,
      data_dir: "/tmp/teamd"
    },
    event_bus: {
      backend: "nats",
      required: true,
      nats_configured: true,
      input_stream: "teamd.input",
      session_stream: "teamd.session",
      delivery_stream: "teamd.delivery",
      task_stream: "teamd.task",
      dlq_stream: "teamd.dlq"
    },
    agents: [],
    sessions: [],
    recent_runs: [],
    recent_tasks: [],
    recent_tool_calls: [],
    delivery_targets: [],
    session_output_routes: [],
    telegram_chats: [],
    recent_traces: [],
    ...overrides
  };
}

test("summarizeOperations counts active and failed runtime entities", () => {
  const summary = summarizeOperations(
    snapshot({
      recent_runs: [
        { id: "run-1", session_id: "s1", status: "running", started_at: 1, updated_at: 2 },
        { id: "run-2", session_id: "s2", status: "failed", started_at: 1, updated_at: 2 }
      ],
      recent_tasks: [
        {
          id: "task-1",
          kind: "agent_task",
          status: "queued",
          context_ref_json: "{}",
          attempt_count: 0,
          max_attempts: 1,
          created_at: 1,
          updated_at: 2
        },
        {
          id: "task-2",
          kind: "delegate",
          status: "cancelled",
          context_ref_json: "{}",
          attempt_count: 1,
          max_attempts: 1,
          created_at: 1,
          updated_at: 2
        }
      ],
      delivery_targets: [{ target_id: "tg-monitoring", kind: "telegram", address: "1", scope: "group", format_policy: "summary", updated_at: 2 }],
      telegram_chats: [{ telegram_chat_id: 1, scope: "group", inbound_queue_mode: "coalesce", updated_at: 2 }]
    })
  );

  assert.equal(summary.activeRuns, 1);
  assert.equal(summary.failedRuns, 1);
  assert.equal(summary.activeTasks, 1);
  assert.equal(summary.failedTasks, 1);
  assert.equal(summary.deliveryTargets, 1);
  assert.equal(summary.telegramInputs, 1);
});

test("recentActiveRuns filters only active states", () => {
  assert.deepEqual(
    recentActiveRuns([
      { id: "run-1", session_id: "s1", status: "queued", started_at: 1, updated_at: 2 },
      { id: "run-2", session_id: "s2", status: "completed", started_at: 1, updated_at: 2 }
    ]).map((run) => run.id),
    ["run-1"]
  );
});
