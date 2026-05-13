import assert from "node:assert/strict";
import { describe, test } from "node:test";
import type { AgentSummary, DeliveryTarget, RunSummary, SessionSummary, TelegramChat } from "../types";
import {
  describeAgentModel,
  describeDeliveryTarget,
  describeRun,
  describeTelegramChat,
  sessionTitle
} from "./entityLabels.ts";

const sessions: SessionSummary[] = [
  {
    id: "session-1111111111111",
    title: "VPN repair",
    agent_profile_id: "default",
    agent_name: "Assistant",
    model: "glm-4.5",
    reasoning_visible: false,
    think_level: "off",
    compactifications: 0,
    auto_approve: true,
    context_tokens: 1200,
    has_pending_approval: false,
    message_count: 8,
    background_job_count: 0,
    running_background_job_count: 0,
    queued_background_job_count: 0,
    created_at: 10,
    updated_at: 30
  },
  {
    id: "session-2222222222222",
    title: "Monitoring room",
    agent_profile_id: "monitor",
    agent_name: "Monitor",
    model: null,
    reasoning_visible: false,
    think_level: null,
    compactifications: 0,
    auto_approve: false,
    context_tokens: 300,
    has_pending_approval: false,
    message_count: 2,
    background_job_count: 0,
    running_background_job_count: 0,
    queued_background_job_count: 0,
    created_at: 12,
    updated_at: 40
  }
];

const agents: AgentSummary[] = [
  {
    id: "default",
    name: "Assistant",
    template_kind: "default",
    updated_at: 30
  },
  {
    id: "monitor",
    name: "Monitor",
    template_kind: "default",
    updated_at: 40
  }
];

describe("entity labels", () => {
  test("sessionTitle prefers title and keeps short id as secondary context", () => {
    assert.deepEqual(sessionTitle("session-1111111111111", sessions), {
      primary: "VPN repair",
      secondary: "session-1111111111111"
    });
  });

  test("describeRun renders run through the owning session, not only raw ids", () => {
    const run: RunSummary = {
      id: "run-chat-session-1111111111111-1770000000",
      session_id: "session-1111111111111",
      status: "running",
      started_at: 1770000000,
      updated_at: 1770000060
    };

    assert.deepEqual(describeRun(run, sessions), {
      primary: "VPN repair",
      secondary: "Assistant · session-1111111111111",
      technical: "run-chat-session-1111111111111-1770000000"
    });
  });

  test("describeTelegramChat derives a readable name from bound session or default agent", () => {
    const chat: TelegramChat = {
      telegram_chat_id: -5263509228,
      scope: "group",
      selected_session_id: "session-2222222222222",
      default_agent_profile_id: "monitor",
      inbound_queue_mode: "coalesce",
      inbound_coalesce_window_ms: 5000,
      updated_at: 55
    };

    assert.deepEqual(describeTelegramChat(chat, sessions, agents), {
      primary: "Monitoring room",
      secondary: "Telegram group · Monitor",
      technical: "-5263509228"
    });
  });

  test("describeDeliveryTarget uses address and agent/session labels when available", () => {
    const target: DeliveryTarget = {
      target_id: "monitoring-chat",
      kind: "telegram",
      address: "-5263509228",
      scope: "monitor",
      format_policy: "summary",
      updated_at: 77
    };

    assert.deepEqual(describeDeliveryTarget(target, sessions, agents), {
      primary: "Telegram -5263509228",
      secondary: "scope: Monitor · format: summary",
      technical: "monitoring-chat"
    });
  });

  test("describeAgentModel surfaces the latest concrete session model", () => {
    assert.equal(describeAgentModel(agents[0], sessions), "glm-4.5");
    assert.equal(describeAgentModel(agents[1], sessions), "runtime default");
  });
});
