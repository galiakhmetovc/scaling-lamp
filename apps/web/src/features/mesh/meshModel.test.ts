import test from "node:test";
import assert from "node:assert/strict";
import { buildAgentLanes, countTasksByStatus, isActiveStatus, isFailedStatus } from "./meshModel.ts";
import type { WebSnapshot } from "../../types.ts";

test("mesh status helpers classify active and failed states", () => {
  assert.equal(isActiveStatus("running"), true);
  assert.equal(isActiveStatus("completed"), false);
  assert.equal(isFailedStatus("failed"), true);
  assert.equal(isFailedStatus("running"), false);
});

test("buildAgentLanes groups sessions, runs, tasks, and outputs by agent", () => {
  const snapshot = {
    agents: [
      { id: "default", name: "Ассистент", template_kind: "default", updated_at: 10 },
      { id: "judge", name: "Judge", template_kind: "judge", updated_at: 11 }
    ],
    sessions: [
      { id: "s1", title: "Main", agent_profile_id: "default", agent_name: "Ассистент", updated_at: 20 },
      { id: "s2", title: "Review", agent_profile_id: "judge", agent_name: "Judge", updated_at: 21 }
    ],
    recent_runs: [
      { id: "r1", session_id: "s1", status: "running", started_at: 22, updated_at: 23 },
      { id: "r2", session_id: "s2", status: "completed", started_at: 24, updated_at: 25 }
    ],
    recent_tasks: [
      {
        id: "task-1",
        kind: "agent_task",
        status: "running",
        source_session_id: "s1",
        owner_agent_id: "default",
        executor_agent_id: "judge",
        context_ref_json: "{}",
        attempt_count: 1,
        max_attempts: 3,
        updated_at: 30,
        created_at: 29
      }
    ],
    telegram_chats: [{ telegram_chat_id: 123, scope: "private", default_agent_profile_id: "default", inbound_queue_mode: "coalesce", updated_at: 31 }],
    delivery_targets: [{ target_id: "telegram-default", kind: "telegram", address: "123", scope: "default", format_policy: "summary", updated_at: 32 }],
    session_output_routes: []
  } as WebSnapshot;

  const lanes = buildAgentLanes(snapshot);

  assert.equal(lanes[0].agent.id, "default");
  assert.equal(lanes[0].sessions.length, 1);
  assert.equal(lanes[0].activeRuns.length, 1);
  assert.equal(lanes[0].tasks.length, 1);
  assert.equal(lanes[0].telegramChats.length, 1);
  assert.equal(lanes[0].deliveryTargets.length, 1);
  assert.equal(lanes[1].agent.id, "judge");
  assert.equal(lanes[1].tasks.length, 1);
});

test("countTasksByStatus returns compact task status counters", () => {
  assert.deepEqual(
    countTasksByStatus([
      { id: "a", kind: "x", status: "running", context_ref_json: "{}", attempt_count: 0, max_attempts: 1, created_at: 1, updated_at: 1 },
      { id: "b", kind: "x", status: "running", context_ref_json: "{}", attempt_count: 0, max_attempts: 1, created_at: 1, updated_at: 1 },
      { id: "c", kind: "x", status: "failed", context_ref_json: "{}", attempt_count: 1, max_attempts: 1, created_at: 1, updated_at: 1 }
    ]),
    { running: 2, failed: 1 }
  );
});
