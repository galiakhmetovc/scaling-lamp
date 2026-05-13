import { strict as assert } from "node:assert";
import { describe, it } from "node:test";
import type { AgentSchedule } from "../../types.ts";
import {
  scheduleDeliveryLabel,
  scheduleModeLabel,
  scheduleStatus,
  scheduleStatusLabel,
  secondsToHuman
} from "./scheduleModel.ts";

function schedule(overrides: Partial<AgentSchedule> = {}): AgentSchedule {
  return {
    id: "daily-status",
    agent_profile_id: "default",
    workspace_root: "/tmp/workspace",
    prompt: "status",
    mode: "interval",
    delivery_mode: "fresh_session",
    target_session_id: null,
    interval_seconds: 900,
    next_fire_at: 100,
    enabled: true,
    created_at: 1,
    updated_at: 2,
    ...overrides
  };
}

describe("scheduleModel", () => {
  it("prioritizes visible error state over enabled flag", () => {
    assert.equal(scheduleStatus(schedule({ last_error: "failed" })), "error");
    assert.equal(scheduleStatusLabel(schedule({ enabled: false })), "выключено");
  });

  it("formats interval and enum labels for operator tables", () => {
    assert.equal(secondsToHuman(900), "15 мин");
    assert.equal(secondsToHuman(7200), "2 ч");
    assert.equal(scheduleModeLabel("after_completion"), "после завершения");
    assert.equal(scheduleDeliveryLabel("existing_session"), "в существующую сессию");
  });
});
